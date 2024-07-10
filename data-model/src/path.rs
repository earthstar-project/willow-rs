use core::borrow::Borrow;
use core::convert::AsRef;
use core::iter;
use core::mem::size_of;
use core::ops::Deref;

use arbitrary::{Arbitrary, Error as ArbitraryError, Unstructured};
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// A [component](https://willowprotocol.org/specs/data-model/index.html#Component) of a Willow Path.
///
/// This type is a thin wrapper around `&'a [u8]` that enforces a const-generic [maximum component length](https://willowprotocol.org/specs/data-model/index.html#max_component_length). Use the `AsRef`, `DeRef`, or `Borrow` implementation to access the immutable byte slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Component<'a, const MAX_COMPONENT_LENGTH: usize>(&'a [u8]);

impl<'a, const MAX_COMPONENT_LENGTH: usize> Component<'a, MAX_COMPONENT_LENGTH> {
    /// Create a `Component` from a byte slice. Return `None` if the slice is longer than `MaxComponentLength`.
    pub fn new(slice: &'a [u8]) -> Option<Self> {
        if slice.len() <= MAX_COMPONENT_LENGTH {
            return Some(unsafe { Self::new_unchecked(slice) }); // Safe because we just checked the length.
        } else {
            return None;
        }
    }

    /// Create a `Component` from a byte slice, without verifying its length.
    pub unsafe fn new_unchecked(slice: &'a [u8]) -> Self {
        Self(slice)
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> Deref for Component<'a, MAX_COMPONENT_LENGTH> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> AsRef<[u8]> for Component<'a, MAX_COMPONENT_LENGTH> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> Borrow<[u8]> for Component<'a, MAX_COMPONENT_LENGTH> {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug)]
/// An error arising from trying to construct a invalid [`Path`] from valid [`PathComponent`].
pub enum InvalidPathError {
    /// The path's total length in bytes is too large.
    PathTooLong,
    /// The path has too many components.
    TooManyComponents,
}
// TODO trait impls

/// An immutable Willow [path](https://willowprotocol.org/specs/data-model/index.html#Path). Thread-safe, cheap to clone, cheap to take prefixes of.
///
/// Enforces that each component has a length of at most `MCL` ([**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length)), that each path has at most `MCC` ([**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count)) components, and that the total size in bytes of all components is at most `MPL` ([**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length)).
pub struct Path<const MCL: usize, const MCC: usize, const MPL: usize> {
    /// The data of the underlying path.
    data: HeapEncoding<MCL>,
    /// Number of components of the `data` to consider for this particular path. Must be less than or equal to the total number of components.
    number_of_components: usize,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Path<MCL, MCC, MPL> {
    /// Construct an empty path, i.e., a path of zero components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn new_empty() -> Self {
        return Path {
            // 16 zero bytes, to work even on platforms on which `usize` has a size of 16.
            data: HeapEncoding(Bytes::from_static(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ])),
            number_of_components: 0,
        };
    }

    /// Construct a singleton path, i.e., a path of exactly one component.
    ///
    /// Copies the bytes of the component into an owned allocation on the heap.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the component. Performs a single allocation of `O(n)` bytes.
    pub fn new_singleton<'a>(comp: Component<'a, MCL>) -> Self {
        let mut buf = BytesMut::with_capacity((2 * size_of::<usize>()) + comp.len());
        buf.extend_from_slice(&(1usize.to_ne_bytes())[..]);
        buf.extend_from_slice(&(comp.len().to_ne_bytes())[..]);
        buf.extend_from_slice(comp.as_ref());

        return Path {
            data: HeapEncoding(buf.freeze()),
            number_of_components: 1,
        };
    }

    /// Construct a path of known total length from an [`ExactSizeIterator`][core::iter::ExactSizeIterator] of components.
    ///
    /// Copies the bytes of the components into an owned allocation on the heap.
    ///
    /// Panics if the claimed `total_length` does not match the sum of the lengths of all the components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the path in bytes. Performs a single allocation of `O(n)` bytes.
    pub fn new_from_iter<'a, I>(total_length: usize, iter: &mut I) -> Result<Self, InvalidPathError>
    where
        I: ExactSizeIterator<Item = Component<'a, MCL>>,
    {
        if total_length > MPL {
            return Err(InvalidPathError::PathTooLong);
        }

        let component_count = iter.len();

        if component_count > MCC {
            return Err(InvalidPathError::TooManyComponents);
        }

        let mut buf =
            BytesMut::with_capacity(((component_count + 1) * size_of::<usize>()) + total_length);
        buf.extend_from_slice(&(component_count.to_ne_bytes())[..]);

        // Fill up the accumulated component lengths with dummy values.
        buf.put_bytes(0, component_count * size_of::<u8>());

        let mut accumulated_component_length = 0;
        for (i, comp) in iter.enumerate() {
            // Overwrite the dummy accumulated component length for this component with the actual value.
            accumulated_component_length += comp.len();
            let start = (1 + i) * size_of::<usize>();
            let end = start + size_of::<usize>();
            buf.as_mut()[start..end]
                .copy_from_slice(&accumulated_component_length.to_ne_bytes()[..]);

            // Append the component to the path.
            buf.extend_from_slice(comp.as_ref());
        }

        if accumulated_component_length != total_length {
            panic!("Tried to construct a path of total length {}, but got components whose accumulated length was {}.", total_length, accumulated_component_length);
        }

        return Ok(Path {
            data: HeapEncoding(buf.freeze()),
            number_of_components: component_count,
        });
    }

    /// Construct a path of from a slice of components.
    ///
    /// Copies the bytes of the components into an owned allocation on the heap.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the path in bytes. Performs a single allocation of `O(n)` bytes.
    pub fn new_from_slice<'a>(components: &[Component<'a, MCL>]) -> Result<Self, InvalidPathError> {
        let mut total_length = 0;
        for comp in components {
            total_length += comp.len();
        }

        return Self::new_from_iter(
            total_length,
            &mut components.iter().map(|comp_ref| *comp_ref),
        );
    }

    /// Construct a new path by appending a component to this one.
    ///
    /// Creates a fully separate copy of the new data on the heap; this function is not more efficient than constructing the new path from scratch.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the new path in bytes. Performs a single allocation of `O(n)` bytes.
    pub fn append<'a>(&self, comp: Component<'a, MCL>) -> Result<Self, InvalidPathError> {
        let total_length = self.get_path_length() + comp.len();
        return Self::new_from_iter(
            total_length,
            &mut ExactLengthChain::new(self.components(), iter::once(comp)),
        );
    }

    /// Construct a new path by appending a slice of components to this one.
    ///
    /// Creates a fully separate copy of the new data on the heap; this function is not more efficient than constructing the new path from scratch.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the new path in bytes. Performs a single allocation of `O(n)` bytes.
    pub fn append_slice<'a>(
        &self,
        components: &[Component<'a, MCL>],
    ) -> Result<Self, InvalidPathError> {
        let mut total_length = self.get_path_length();
        for comp in components {
            total_length += comp.len();
        }

        return Self::new_from_iter(
            total_length,
            &mut ExactLengthChain::new(
                self.components(),
                components.iter().map(|comp_ref| *comp_ref),
            ),
        );
    }

    /// Get the number of components in this path.
    ///
    /// Guaranteed to be at most `MCC`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn get_component_count(&self) -> usize {
        return self.number_of_components;
    }

    /// Return whether this path has zero components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn is_empty(&self) -> bool {
        self.get_component_count() == 0
    }

    /// Get the sum of the lengths of all components in this path.
    ///
    /// Guaranteed to be at most `MCC`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn get_path_length(&self) -> usize {
        if self.number_of_components == 0 {
            return 0;
        } else {
            return self
                .data
                .get_sum_of_lengths_for_component(self.number_of_components - 1)
                .unwrap();
        }
    }

    /// Get the `i`-th [`Component`] of this path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn get_component<'s>(&'s self, i: usize) -> Option<Component<'s, MCL>> {
        return self.data.get_component(i);
    }

    /// Create an iterator over the components of this path.
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn components<'s>(
        &'s self,
    ) -> impl DoubleEndedIterator<Item = Component<'s, MCL>> + ExactSizeIterator<Item = Component<'s, MCL>>
    {
        (0..self.get_component_count()).map(|i| {
            self.get_component(i).unwrap() // Only `None` if `i >= self.get_component_count()`
        })
    }
}

/// Efficient, heap-allocated storage of a path encoding:
///
/// - First, a usize that gives the total number of path components.
/// - Second, that many usizes, where the i-th one gives the sum of the lengths of the first i components.
/// - Third, the concatenation of all components.
///
/// Note that these are not guaranteed to fulfil alignment requirements of usize, so we need to be careful in how we access these.
/// Always use the methods on this struct for that reason.
struct HeapEncoding<const MAX_COMPONENT_LENGTH: usize>(Bytes);

// All offsets are in bytes, unless otherwise specified.
// Arguments named `i` are the index of a component in the string, *not* a byte offset.
impl<const MAX_COMPONENT_LENGTH: usize> HeapEncoding<MAX_COMPONENT_LENGTH> {
    fn get_component_count(&self) -> usize {
        self.get_usize_at_offset(0).unwrap() // We always store at least 8byte for the component count.
    }

    // None if i is outside the slice.
    fn get_sum_of_lengths_for_component(&self, i: usize) -> Option<usize> {
        let start_offset_in_bytes = Self::start_offset_of_sum_of_lengths_for_component(i);
        self.get_usize_at_offset(start_offset_in_bytes)
    }

    fn get_component<'s>(&'s self, i: usize) -> Option<Component<'s, MAX_COMPONENT_LENGTH>> {
        let start = self.start_offset_of_component(i)?;
        let end = self.end_offset_of_component(i)?;
        return Some(unsafe { Component::new_unchecked(&self.0[start..end]) });
    }

    fn start_offset_of_sum_of_lengths_for_component(i: usize) -> usize {
        // First usize is the number of components, then the i usizes storing the lengths; hence at i + 1.
        size_of::<usize>() * (i + 1)
    }

    fn start_offset_of_component(&self, i: usize) -> Option<usize> {
        let metadata_length = (self.get_component_count() + 1) * size_of::<usize>();
        if i == 0 {
            return Some(metadata_length);
        } else {
            return self
                .get_sum_of_lengths_for_component(i - 1) // Length of everything up until the previous component.
                .map(|length| length + metadata_length);
        }
    }

    fn end_offset_of_component(&self, i: usize) -> Option<usize> {
        let metadata_length = (self.get_component_count() + 1) * size_of::<usize>();
        return self
            .get_sum_of_lengths_for_component(i)
            .map(|length| length + metadata_length);
    }

    fn get_usize_at_offset(&self, offset_in_bytes: usize) -> Option<usize> {
        let end = offset_in_bytes + size_of::<usize>();

        // We cannot interpret the memory in the slice as a usize directly, because the alignment might not match.
        // So we first copy the bytes onto the heap, then construct a usize from it.
        let mut usize_bytes = [0u8; size_of::<usize>()];

        if self.0.len() <= end {
            return None;
        } else {
            usize_bytes.copy_from_slice(&self.0[offset_in_bytes..end]);
            return Some(usize::from_ne_bytes(usize_bytes));
        }
    }
}

// #[derive(Debug)]
// /// An error indicating a [`PathComponent`'s bytestring is too long.
// pub struct ComponentTooLongError;

// /// A bytestring representing an individual component of a [`Path`]. Provides access to the raw bytes via the [`AsRef<u8>`] trait.
// ///
// /// ## Implementation notes
// ///
// /// - This trait provides an immutable interface, so cloning of a [`PathComponent`] implementation is expected to be cheap.
// /// - Implementations of the [`Ord`] trait must correspond to lexicographically comparing the AsRef bytes.
// pub trait PathComponent: Eq + AsRef<[u8]> + Clone + PartialOrd + Ord {
//     /// The maximum bytelength of a path component, corresponding to Willow's [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_component_length) parameter.
//     const MAX_COMPONENT_LENGTH: usize;

//     /// Construct a new [`PathComponent`] from the provided slice, or return a [`ComponentTooLongError`] if the resulting component would be longer than [`PathComponent::MAX_COMPONENT_LENGTH`].
//     fn new(components: &[u8]) -> Result<Self, ComponentTooLongError>;

//     /// Construct a new [`PathComponent`] from the concatenation of `head` and the `tail`, or return a [`ComponentTooLongError`] if the resulting component would be longer than [`PathComponent::MAX_COMPONENT_LENGTH`].
//     ///
//     /// This operation occurs when computing prefix successors, and the default implementation needs to perform an allocation. Implementers of this trait can override this with something more efficient if possible.
//     fn new_with_tail(head: &[u8], tail: u8) -> Result<Self, ComponentTooLongError> {
//         let mut vec = Vec::with_capacity(head.len() + 1);
//         vec.extend_from_slice(head);
//         vec.push(tail);

//         Self::new(&vec)
//     }

//     /// Return a new [`PathComponent`] which corresponds to the empty string.
//     fn empty() -> Self {
//         Self::new(&[]).unwrap()
//     }

//     /// The length of the component's `as_ref` bytes.
//     fn len(&self) -> usize {
//         self.as_ref().len()
//     }

//     /// Whether the component is an empty bytestring or not.
//     fn is_empty(&self) -> bool {
//         self.len() == 0
//     }

//     /// Try to append a zero byte to the end of the component.
//     /// Return `None` if the resulting component would be too long.
//     fn try_append_zero_byte(&self) -> Option<Self> {
//         if self.len() == Self::MAX_COMPONENT_LENGTH {
//             return None;
//         }

//         let mut new_component_vec = Vec::with_capacity(self.len() + 1);

//         new_component_vec.extend_from_slice(self.as_ref());
//         new_component_vec.push(0);

//         Some(Self::new(&new_component_vec).unwrap())
//     }

//     /// Create a new copy which differs at index `i`.
//     /// Implementers may panic if `i` is out of bound.
//     fn set_byte(&self, i: usize, value: u8) -> Self;

//     /// Interpret the component as a binary number, and increment that number by 1.
//     /// If doing so would increase the bytelength of the component, return `None`.
//     fn try_increment_fixed_width(&self) -> Option<Self> {
//         // Wish we could avoid this allocation somehow.
//         let mut new_component = self.clone();

//         for i in (0..self.len()).rev() {
//             let byte = self.as_ref()[i];

//             if byte == 255 {
//                 new_component = new_component.set_byte(i, 0);
//             } else {
//                 return Some(new_component.set_byte(i, byte + 1));
//             }
//         }

//         None
//     }

//     /// Return the least component which is greater than `self` but which is not prefixed by `self`.
//     fn prefix_successor(&self) -> Option<Self> {
//         for i in (0..self.len()).rev() {
//             if self.as_ref()[i] != 255 {
//                 // Since we are not adjusting the length of the component this will always succeed.
//                 return Some(
//                     Self::new_with_tail(&self.as_ref()[0..i], self.as_ref()[i] + 1).unwrap(),
//                 );
//             }
//         }

//         None
//     }
// }

// /// A sequence of [`PathComponent`], used to name and query entries in [Willow's data model](https://willowprotocol.org/specs/data-model/index.html#data_model).
// ///
// pub trait Path: PartialEq + Eq + PartialOrd + Ord + Clone {
//     type Component: PathComponent;

//     /// The maximum number of [`PathComponent`] a [`Path`] may have, corresponding to Willow's [`max_component_count`](https://willowprotocol.org/specs/data-model/index.html#max_component_count) parameter
//     const MAX_COMPONENT_COUNT: usize;
//     /// The maximum total number of bytes a [`Path`] may have, corresponding to Willow's [`max_path_length`](https://willowprotocol.org/specs/data-model/index.html#max_path_length) parameter
//     const MAX_PATH_LENGTH: usize;

//     /// Contruct a new [`Path`] from a slice of [`PathComponent`], or return a [`InvalidPathError`] if the resulting path would be too long or have too many components.
//     fn new(components: &[Self::Component]) -> Result<Self, InvalidPathError>;

//     /// Construct a new [`Path`] with no components.
//     fn empty() -> Self;

//     /// Return a new [`Path`] with the components of this path suffixed with the given [`PathComponent`](PathComponent), or return [`InvalidPathError`] if the resulting path would be invalid in some way.
//     fn append(&self, component: Self::Component) -> Result<Self, InvalidPathError>;

//     /// Return an iterator of all this path's components.
//     ///
//     /// The `.len` method of the returned [`ExactSizeIterator`] must coincide with [`Self::component_count`].
//     fn components(
//         &self,
//     ) -> impl DoubleEndedIterator<Item = &Self::Component> + ExactSizeIterator<Item = &Self::Component>;

//     /// Return the number of [`PathComponent`] in the path.
//     fn component_count(&self) -> usize;

//     /// Return whether the path has any [`PathComponent`] or not.
//     fn is_empty(&self) -> bool {
//         self.component_count() == 0
//     }

//     /// Create a new [`Path`] by taking the first `length` components of this path.
//     fn create_prefix(&self, length: usize) -> Self;

//     /// Return all possible prefixes of a path, including the empty path and the path itself.
//     fn all_prefixes(&self) -> impl Iterator<Item = Self> {
//         let self_len = self.components().count();

//         (0..=self_len).map(|i| self.create_prefix(i))
//     }

//     /// Test whether this path is a prefix of the given path.
//     /// Paths are always a prefix of themselves.
//     fn is_prefix_of(&self, other: &Self) -> bool {
//         for (comp_a, comp_b) in self.components().zip(other.components()) {
//             if comp_a != comp_b {
//                 return false;
//             }
//         }

//         self.component_count() <= other.component_count()
//     }

//     /// Test whether this path is prefixed by the given path.
//     /// Paths are always a prefix of themselves.
//     fn is_prefixed_by(&self, other: &Self) -> bool {
//         other.is_prefix_of(self)
//     }

//     /// Return the longest common prefix of this path and the given path.
//     fn longest_common_prefix(&self, other: &Self) -> Self {
//         let mut lcp_len = 0;

//         for (comp_a, comp_b) in self.components().zip(other.components()) {
//             if comp_a != comp_b {
//                 break;
//             }

//             lcp_len += 1
//         }

//         self.create_prefix(lcp_len)
//     }

//     /// Return the least path which is greater than `self`, or return `None` if `self` is the greatest possible path.
//     fn successor(&self) -> Option<Self> {
//         if self.component_count() == 0 {
//             let new_component = Self::Component::new(&[]).ok()?;
//             return Self::new(&[new_component]).ok();
//         }

//         // Try and add an empty component.
//         if let Ok(path) = self.append(Self::Component::empty()) {
//             return Some(path);
//         }

//         for (i, component) in self.components().enumerate().rev() {
//             // Try and do the *next* simplest thing (add a 0 byte to the component).
//             if let Some(component) = component.try_append_zero_byte() {
//                 if let Ok(path) = self.create_prefix(i).append(component) {
//                     return Some(path);
//                 }
//             }

//             // Otherwise we need to increment the component fixed-width style!
//             if let Some(incremented_component) = component.try_increment_fixed_width() {
//                 // We can unwrap here because neither the max path length, component count, or component length has changed.
//                 return Some(self.create_prefix(i).append(incremented_component).unwrap());
//             }
//         }

//         None
//     }

//     /// Return the least path that is greater than `self` and which is not prefixed by `self`, or `None` if `self` is the empty path *or* if `self` is the greatest path.
//     fn successor_of_prefix(&self) -> Option<Self> {
//         for (i, component) in self.components().enumerate().rev() {
//             if let Some(successor_comp) = component.try_append_zero_byte() {
//                 if let Ok(path) = self.create_prefix(i).append(successor_comp) {
//                     return Some(path);
//                 }
//             }

//             if let Some(successor_comp) = component.prefix_successor() {
//                 return self.create_prefix(i).append(successor_comp).ok();
//             }
//         }

//         None
//     }
// }

// #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
// pub struct PathComponentBox<const MCL: usize>(Box<[u8]>);

// /// An implementation of [`PathComponent`] for [`PathComponentBox`].
// ///
// /// ## Type parameters
// ///
// /// - `MCL`: A [`usize`] used as [`PathComponent::MAX_COMPONENT_LENGTH`].
// impl<const MCL: usize> PathComponent for PathComponentBox<MCL> {
//     const MAX_COMPONENT_LENGTH: usize = MCL;

//     /// Create a new component by cloning and appending all bytes from the slice into a [`Vec<u8>`], or return a [`ComponentTooLongError`] if the bytelength of the slice exceeds [`PathComponent::MAX_COMPONENT_LENGTH`].
//     fn new(bytes: &[u8]) -> Result<Self, ComponentTooLongError> {
//         if bytes.len() > Self::MAX_COMPONENT_LENGTH {
//             return Err(ComponentTooLongError);
//         }

//         Ok(Self(bytes.into()))
//     }

//     fn len(&self) -> usize {
//         self.0.len()
//     }

//     fn is_empty(&self) -> bool {
//         self.0.is_empty()
//     }

//     fn set_byte(&self, i: usize, value: u8) -> Self {
//         let mut new_component = self.clone();

//         new_component.0[i] = value;

//         new_component
//     }
// }

// impl<const MCL: usize> AsRef<[u8]> for PathComponentBox<MCL> {
//     fn as_ref(&self) -> &[u8] {
//         self.0.as_ref()
//     }
// }

// impl<'a, const MCL: usize> Arbitrary<'a> for PathComponentBox<MCL> {
//     fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, ArbitraryError> {
//         let boxx: Box<[u8]> = Arbitrary::arbitrary(u)?;
//         Self::new(&boxx).map_err(|_| ArbitraryError::IncorrectFormat)
//     }

//     #[inline]
//     fn size_hint(depth: usize) -> (usize, Option<usize>) {
//         <Box<[u8]> as Arbitrary<'a>>::size_hint(depth)
//     }
// }

// #[derive(Debug, PartialEq, Eq, Clone)]
// /// A cheaply cloneable [`Path`] using a `Rc<[PathComponentBox]>`.
// /// While cloning is cheap, operations which return modified forms of the path (e.g. [`Path::append`]) are not, as they have to clone and adjust the contents of the underlying [`Rc`].
// pub struct PathRc<const MCL: usize, const MCC: usize, const MPL: usize>(
//     Rc<[PathComponentBox<MCL>]>,
// );

// impl<const MCL: usize, const MCC: usize, const MPL: usize> Path for PathRc<MCL, MCC, MPL> {
//     type Component = PathComponentBox<MCL>;

//     const MAX_COMPONENT_COUNT: usize = MCC;
//     const MAX_PATH_LENGTH: usize = MPL;

//     fn new(components: &[Self::Component]) -> Result<Self, InvalidPathError> {
//         if components.len() > Self::MAX_COMPONENT_COUNT {
//             return Err(InvalidPathError::TooManyComponents);
//         };

//         let mut path_vec = Vec::new();
//         let mut total_length = 0;

//         for component in components {
//             total_length += component.len();

//             if total_length > Self::MAX_PATH_LENGTH {
//                 return Err(InvalidPathError::PathTooLong);
//             } else {
//                 path_vec.push(component.clone());
//             }
//         }

//         Ok(PathRc(path_vec.into()))
//     }

//     fn empty() -> Self {
//         PathRc(Vec::new().into())
//     }

//     fn create_prefix(&self, length: usize) -> Self {
//         if length == 0 {
//             return Self::empty();
//         }

//         let until = core::cmp::min(length, self.0.len());
//         let slice = &self.0[0..until];

//         Path::new(slice).unwrap()
//     }

//     fn append(&self, component: Self::Component) -> Result<Self, InvalidPathError> {
//         let total_component_count = self.0.len();

//         if total_component_count + 1 > MCC {
//             return Err(InvalidPathError::TooManyComponents);
//         }

//         let total_path_length = self.0.iter().fold(0, |acc, item| acc + item.0.len());

//         if total_path_length + component.as_ref().len() > MPL {
//             return Err(InvalidPathError::PathTooLong);
//         }

//         let mut new_path_vec = Vec::new();

//         for component in self.components() {
//             new_path_vec.push(component.clone())
//         }

//         new_path_vec.push(component);

//         Ok(PathRc(new_path_vec.into()))
//     }

//     fn components(
//         &self,
//     ) -> impl DoubleEndedIterator<Item = &Self::Component> + ExactSizeIterator<Item = &Self::Component>
//     {
//         return self.0.iter();
//     }

//     fn component_count(&self) -> usize {
//         self.0.len()
//     }
// }

// impl<const MCL: usize, const MCC: usize, const MPL: usize> Ord for PathRc<MCL, MCC, MPL> {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         for (my_comp, your_comp) in self.components().zip(other.components()) {
//             let comparison = my_comp.cmp(your_comp);

//             match comparison {
//                 std::cmp::Ordering::Equal => { /* Continue */ }
//                 _ => return comparison,
//             }
//         }

//         self.component_count().cmp(&other.component_count())
//     }
// }

// impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialOrd for PathRc<MCL, MCC, MPL> {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         Some(self.cmp(other))
//     }
// }

// impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
//     for PathRc<MCL, MCC, MPL>
// {
//     fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, ArbitraryError> {
//         let boxx: Box<[PathComponentBox<MCL>]> = Arbitrary::arbitrary(u)?;
//         Self::new(&boxx).map_err(|_| ArbitraryError::IncorrectFormat)
//     }

//     #[inline]
//     fn size_hint(depth: usize) -> (usize, Option<usize>) {
//         <Box<[u8]> as Arbitrary<'a>>::size_hint(depth)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use std::cmp::Ordering::Less;

//     use super::*;

//     const MCL: usize = 8;
//     const MCC: usize = 4;
//     const MPL: usize = 16;

//     #[test]
//     fn empty() {
//         let empty_path = PathRc::<MCL, MCC, MPL>::empty();

//         assert_eq!(empty_path.components().count(), 0);
//     }

//     #[test]
//     fn new() {
//         let component_too_long =
//             PathComponentBox::<MCL>::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'z']);

//         assert!(matches!(component_too_long, Err(ComponentTooLongError)));

//         let too_many_components = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'z']).unwrap(),
//         ]);

//         assert!(matches!(
//             too_many_components,
//             Err(InvalidPathError::TooManyComponents)
//         ));

//         let path_too_long = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']).unwrap(),
//             PathComponentBox::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']).unwrap(),
//             PathComponentBox::new(&[b'z']).unwrap(),
//         ]);

//         assert!(matches!(path_too_long, Err(InvalidPathError::PathTooLong)));
//     }

//     #[test]
//     fn append() {
//         let path = PathRc::<MCL, MCC, MPL>::empty();

//         let r1 = path.append(PathComponentBox::new(&[b'a']).unwrap());
//         assert!(r1.is_ok());
//         let p1 = r1.unwrap();
//         assert_eq!(p1.components().count(), 1);

//         let r2 = p1.append(PathComponentBox::new(&[b'b']).unwrap());
//         assert!(r2.is_ok());
//         let p2 = r2.unwrap();
//         assert_eq!(p2.components().count(), 2);

//         let r3 = p2.append(PathComponentBox::new(&[b'c']).unwrap());
//         assert!(r3.is_ok());
//         let p3 = r3.unwrap();
//         assert_eq!(p3.components().count(), 3);

//         let r4 = p3.append(PathComponentBox::new(&[b'd']).unwrap());
//         assert!(r4.is_ok());
//         let p4 = r4.unwrap();
//         assert_eq!(p4.components().count(), 4);

//         let r5 = p4.append(PathComponentBox::new(&[b'z']).unwrap());
//         assert!(r5.is_err());

//         let collected = p4
//             .components()
//             .map(|comp| comp.as_ref())
//             .collect::<Vec<&[u8]>>();

//         assert_eq!(collected, vec![[b'a'], [b'b'], [b'c'], [b'd'],])
//     }

//     #[test]
//     fn prefix() {
//         let path = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let prefix0 = path.create_prefix(0);

//         assert_eq!(prefix0, PathRc::empty());

//         let prefix1 = path.create_prefix(1);

//         assert_eq!(
//             prefix1,
//             PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap()
//         );

//         let prefix2 = path.create_prefix(2);

//         assert_eq!(
//             prefix2,
//             PathRc::<MCL, MCC, MPL>::new(&[
//                 PathComponentBox::new(&[b'a']).unwrap(),
//                 PathComponentBox::new(&[b'b']).unwrap()
//             ])
//             .unwrap()
//         );

//         let prefix3 = path.create_prefix(3);

//         assert_eq!(
//             prefix3,
//             PathRc::<MCL, MCC, MPL>::new(&[
//                 PathComponentBox::new(&[b'a']).unwrap(),
//                 PathComponentBox::new(&[b'b']).unwrap(),
//                 PathComponentBox::new(&[b'c']).unwrap()
//             ])
//             .unwrap()
//         );

//         let prefix4 = path.create_prefix(4);

//         assert_eq!(
//             prefix4,
//             PathRc::<MCL, MCC, MPL>::new(&[
//                 PathComponentBox::new(&[b'a']).unwrap(),
//                 PathComponentBox::new(&[b'b']).unwrap(),
//                 PathComponentBox::new(&[b'c']).unwrap()
//             ])
//             .unwrap()
//         )
//     }

//     #[test]
//     fn prefixes() {
//         let path = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let prefixes: Vec<PathRc<MCL, MCC, MPL>> = path.all_prefixes().collect();

//         assert_eq!(
//             prefixes,
//             vec![
//                 PathRc::<MCL, MCC, MPL>::new(&[]).unwrap(),
//                 PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
//                 PathRc::<MCL, MCC, MPL>::new(&[
//                     PathComponentBox::new(&[b'a']).unwrap(),
//                     PathComponentBox::new(&[b'b']).unwrap()
//                 ])
//                 .unwrap(),
//                 PathRc::<MCL, MCC, MPL>::new(&[
//                     PathComponentBox::new(&[b'a']).unwrap(),
//                     PathComponentBox::new(&[b'b']).unwrap(),
//                     PathComponentBox::new(&[b'c']).unwrap()
//                 ])
//                 .unwrap(),
//             ]
//         )
//     }

//     #[test]
//     fn is_prefix_of() {
//         let path_a = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//         ])
//         .unwrap();

//         let path_b = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let path_c = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'x']).unwrap(),
//             PathComponentBox::new(&[b'y']).unwrap(),
//             PathComponentBox::new(&[b'z']).unwrap(),
//         ])
//         .unwrap();

//         assert!(path_a.is_prefix_of(&path_b));
//         assert!(!path_a.is_prefix_of(&path_c));

//         let path_d = PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[]).unwrap()]).unwrap();
//         let path_e = PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[0]).unwrap()]).unwrap();

//         assert!(!path_d.is_prefix_of(&path_e));

//         let empty_path = PathRc::empty();

//         assert!(empty_path.is_prefix_of(&path_d));
//     }

//     #[test]
//     fn is_prefixed_by() {
//         let path_a = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//         ])
//         .unwrap();

//         let path_b = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let path_c = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'x']).unwrap(),
//             PathComponentBox::new(&[b'y']).unwrap(),
//             PathComponentBox::new(&[b'z']).unwrap(),
//         ])
//         .unwrap();

//         assert!(path_b.is_prefixed_by(&path_a));
//         assert!(!path_c.is_prefixed_by(&path_a));
//     }

//     #[test]
//     fn longest_common_prefix() {
//         let path_a = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'x']).unwrap(),
//         ])
//         .unwrap();

//         let path_b = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'b']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let path_c = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'x']).unwrap(),
//             PathComponentBox::new(&[b'y']).unwrap(),
//             PathComponentBox::new(&[b'z']).unwrap(),
//         ])
//         .unwrap();

//         let lcp_a_b = path_a.longest_common_prefix(&path_b);

//         assert_eq!(
//             lcp_a_b,
//             PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap()
//         );

//         let lcp_b_a = path_b.longest_common_prefix(&path_a);

//         assert_eq!(lcp_b_a, lcp_a_b);

//         let lcp_a_x = path_a.longest_common_prefix(&path_c);

//         assert_eq!(lcp_a_x, PathRc::empty());

//         let path_d = PathRc::<MCL, MCC, MPL>::new(&[
//             PathComponentBox::new(&[b'a']).unwrap(),
//             PathComponentBox::new(&[b'x']).unwrap(),
//             PathComponentBox::new(&[b'c']).unwrap(),
//         ])
//         .unwrap();

//         let lcp_b_d = path_b.longest_common_prefix(&path_d);

//         assert_eq!(
//             lcp_b_d,
//             PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap()
//         )
//     }

//     fn make_test_path<const MCL: usize, const MCC: usize, const MPL: usize>(
//         vector: &Vec<Vec<u8>>,
//     ) -> PathRc<MCL, MCC, MPL> {
//         let components: Vec<_> = vector
//             .iter()
//             .map(|bytes_vec| PathComponentBox::new(bytes_vec).expect("the component was too long"))
//             .collect();

//         PathRc::new(&components).expect("the path was invalid")
//     }

//     #[test]
//     fn ordering() {
//         let test_vector = vec![(vec![vec![0, 0], vec![]], vec![vec![0, 0, 0]], Less)];

//         for (a, b, expected) in test_vector {
//             let path_a: PathRc<3, 3, 3> = make_test_path(&a);
//             let path_b: PathRc<3, 3, 3> = make_test_path(&b);

//             let ordering = path_a.cmp(&path_b);

//             if ordering != expected {
//                 println!("a: {:?}", a);
//                 println!("b: {:?}", b);

//                 assert_eq!(ordering, expected);
//             }
//         }
//     }
// }

/// Like core::iter::Chain, but implements ExactSizeIter if both components implement it. Panics if the resulting length overflows.
///
/// Code liberally copy-pasted from the standard library.
pub struct ExactLengthChain<A, B> {
    a: Option<A>,
    b: Option<B>,
}

impl<A, B> ExactLengthChain<A, B> {
    fn new(a: A, b: B) -> ExactLengthChain<A, B> {
        ExactLengthChain {
            a: Some(a),
            b: Some(b),
        }
    }
}

impl<A, B> Iterator for ExactLengthChain<A, B>
where
    A: Iterator,
    B: Iterator<Item = A::Item>,
{
    type Item = A::Item;

    #[inline]
    fn next(&mut self) -> Option<A::Item> {
        and_then_or_clear(&mut self.a, Iterator::next).or_else(|| self.b.as_mut()?.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower_a, higher_a) = self.a.as_ref().map_or((0, None), |a| a.size_hint());
        let (lower_b, higher_b) = self.b.as_ref().map_or((0, None), |b| b.size_hint());

        let higher = match (higher_a, higher_b) {
            (Some(a), Some(b)) => Some(
                a.checked_add(b)
                    .expect("Some of lengths of two iterators must not overflow."),
            ),
            _ => None,
        };

        return (lower_a + lower_b, higher);
    }
}

impl<A, B> DoubleEndedIterator for ExactLengthChain<A, B>
where
    A: DoubleEndedIterator,
    B: DoubleEndedIterator<Item = A::Item>,
{
    #[inline]
    fn next_back(&mut self) -> Option<A::Item> {
        and_then_or_clear(&mut self.b, |b| b.next_back()).or_else(|| self.a.as_mut()?.next_back())
    }
}

impl<A, B> ExactSizeIterator for ExactLengthChain<A, B>
where
    A: ExactSizeIterator,
    B: ExactSizeIterator<Item = A::Item>,
{
}

#[inline]
fn and_then_or_clear<T, U>(opt: &mut Option<T>, f: impl FnOnce(&mut T) -> Option<U>) -> Option<U> {
    let x = f(opt.as_mut()?);
    if x.is_none() {
        *opt = None;
    }
    x
}
