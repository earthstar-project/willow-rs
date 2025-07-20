#[cfg(feature = "dev")]
use arbitrary::{size_hint::and_all, Arbitrary, Error as ArbitraryError, Unstructured};
use ufotofu_codec::Blame;

// The `Path` struct is tested in `fuzz/path.rs`, `fuzz/path2.rs`, `fuzz/path3.rs`, `fuzz/path3.rs` by comparing against a non-optimised reference implementation.
// Further, the `successor` and `greater_but_not_prefixed` methods of that reference implementation are tested in `fuzz/path_successor.rs` and friends, and `fuzz/path_successor_of_prefix.rs` and friends.

use core::convert::AsRef;
use core::fmt::Debug;
use core::hash::Hash;
use std::mem::size_of;

use bytes::{BufMut, Bytes, BytesMut};

mod builder;
pub use builder::PathBuilder;

mod representation;
use representation::Representation;

mod component;
pub use component::*;

mod codec; // Nothing to import, the file only provides trait implementations.
pub use codec::{
    decode_path_extends_path, decode_path_extends_path_canonic, encode_path_extends_path,
    path_extends_path_encoding_len,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// An error arising from trying to construct a invalid [`Path`] from valid components.
pub enum InvalidPathError {
    /// The path's total length in bytes is too large.
    PathTooLong,
    /// The path has too many components.
    TooManyComponents,
}

impl core::fmt::Display for InvalidPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidPathError::PathTooLong => {
                write!(
                    f,
                    "Total length of a path in bytes exceeded the maximum path length"
                )
            }
            InvalidPathError::TooManyComponents => {
                write!(
                    f,
                    "Number of components of a path exceeded the maximum component count"
                )
            }
        }
    }
}

impl std::error::Error for InvalidPathError {}

impl From<InvalidPathError> for Blame {
    fn from(_value: InvalidPathError) -> Self {
        Blame::TheirFault
    }
}

/// Raised upon failing to construct a [`Path`] from bytes that each may or may not exceeded the maximum component length.
#[derive(Debug, Clone)]
pub enum PathConstructionError {
    InvalidPath(InvalidPathError),
    ComponentTooLongError,
}

impl std::error::Error for PathConstructionError {}
impl std::fmt::Display for PathConstructionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathConstructionError::InvalidPath(invalid_path_error) => {
                std::fmt::Display::fmt(invalid_path_error, f)
            }
            PathConstructionError::ComponentTooLongError => {
                write!(
                    f,
                    "Length of a path component in bytes exceeded the maximum component length"
                )
            }
        }
    }
}

impl From<InvalidPathError> for PathConstructionError {
    fn from(value: InvalidPathError) -> Self {
        Self::InvalidPath(value)
    }
}

/// An immutable Willow [path](https://willowprotocol.org/specs/data-model/index.html#Path). Thread-safe, cheap to clone, cheap to take prefixes of, expensive to append to (linear time complexity).
///
/// Enforces that each component has a length of at most `MCL` ([**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length)), that each path has at most `MCC` ([**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count)) components, and that the total size in bytes of all components is at most `MPL` ([**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length)).
#[derive(Clone)]
pub struct Path<const MCL: usize, const MCC: usize, const MPL: usize> {
    /// The data of the underlying path.
    data: Bytes,
    /// Number of components of the `data` to consider for this particular path (starting from the first component). Must be less than or equal to the total number of components.
    /// This field enables cheap prefix creation by cloning the heap data (which is reference counted) and adjusting the `component_count`.
    component_count: usize,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Path<MCL, MCC, MPL> {
    /// Returns an empty path, i.e., a path of zero components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn new_empty() -> Self {
        PathBuilder::new(0, 0)
            .expect("The empty path is legal for every choice of of MCL, MCC, and MPL.")
            .build()
    }

    /// Creates a singleton path, i.e., a path of exactly one component.
    ///
    /// Copies the bytes of the component into an owned allocation on the heap.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the component. Performs a single allocation of `O(n)` bytes.
    pub fn new_singleton(comp: Component<MCL>) -> Result<Self, InvalidPathError> {
        let mut builder = PathBuilder::new(comp.len(), 1)?;
        builder.append_component(comp);
        Ok(builder.build())
    }

    /// Creates a path of known total length from an [`ExactSizeIterator`] of components.
    ///
    /// Copies the bytes of the components into an owned allocation on the heap.
    ///
    /// Panics if the claimed `total_length` does not match the sum of the lengths of all the components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn new_from_iter<'a, I>(total_length: usize, iter: &mut I) -> Result<Self, InvalidPathError>
    where
        I: ExactSizeIterator<Item = Component<'a, MCL>>,
    {
        let mut builder = PathBuilder::new(total_length, iter.len())?;

        for component in iter {
            builder.append_component(component);
        }

        Ok(builder.build())
    }

    /// Creates a path from a slice of components.
    ///
    /// Copies the bytes of the components into an owned allocation on the heap.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn new_from_slice(components: &[Component<MCL>]) -> Result<Self, InvalidPathError> {
        let mut total_length = 0;
        for comp in components {
            total_length += comp.len();
        }

        Self::new_from_iter(total_length, &mut components.iter().copied())
    }

    /// Creates a new path by appending a component to this one.
    ///
    /// Creates a fully separate copy of the new data on the heap; this function is not more efficient than constructing the new path from scratch.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn append(&self, comp: Component<MCL>) -> Result<Self, InvalidPathError> {
        let mut builder =
            PathBuilder::new(self.path_length() + comp.len(), self.component_count() + 1)?;

        for component in self.components() {
            builder.append_component(component);
        }
        builder.append_component(comp);

        Ok(builder.build())
    }

    /// Creates a new path by appending a slice of components to this one.
    ///
    /// Creates a fully separate copy of the new data on the heap; this function is not more efficient than constructing the new path from scratch.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn append_slice(&self, components: &[Component<MCL>]) -> Result<Self, InvalidPathError> {
        let mut total_length = self.path_length();
        for comp in components {
            total_length += comp.len();
        }

        let mut builder = PathBuilder::new_from_prefix(
            total_length,
            self.component_count() + components.len(),
            self,
            self.component_count(),
        )?;

        for additional_component in components {
            builder.append_component(*additional_component);
        }

        Ok(builder.build())
    }

    /// Returns the number of components in this path.
    ///
    /// Guaranteed to be at most `MCC`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn component_count(&self) -> usize {
        self.component_count
    }

    /// Returns whether this path has zero components.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn is_empty(&self) -> bool {
        self.component_count() == 0
    }

    /// Returns the sum of the lengths of all components in this path.
    ///
    /// Guaranteed to be at most `MCC`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn path_length(&self) -> usize {
        self.path_length_of_prefix(self.component_count())
    }

    /// Returns the `i`-th [`Component`] of this path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn component(&self, i: usize) -> Option<Component<MCL>> {
        if i < self.component_count {
            Some(Representation::component(&self.data, i))
        } else {
            None
        }
    }

    /// Returns an owned handle to the `i`-th [`Component`] of this path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn owned_component(&self, i: usize) -> Option<OwnedComponent<MCL>> {
        if i < self.component_count {
            let start = Representation::start_offset_of_component(&self.data, i);
            let end = Representation::end_offset_of_component(&self.data, i);
            Some(OwnedComponent(self.data.slice(start..end)))
        } else {
            None
        }
    }

    /// Creates an iterator over the components of this path.
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn components(
        &self,
    ) -> impl DoubleEndedIterator<Item = Component<MCL>> + ExactSizeIterator<Item = Component<MCL>>
    {
        self.suffix_components(0)
    }

    /// Creates an iterator over the components of this path, starting at the `i`-th component. If `i` is greater than or equal to the number of components, the iterator yields zero items.
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn suffix_components(
        &self,
        i: usize,
    ) -> impl DoubleEndedIterator<Item = Component<MCL>> + ExactSizeIterator<Item = Component<MCL>>
    {
        (i..self.component_count()).map(|i| {
            self.component(i).unwrap() // Only `None` if `i >= self.component_count()`
        })
    }

    /// Creates an iterator over owned handles to the components of this path.
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn owned_components(
        &self,
    ) -> impl DoubleEndedIterator<Item = OwnedComponent<MCL>>
           + ExactSizeIterator<Item = OwnedComponent<MCL>>
           + '_ {
        self.suffix_owned_components(0)
    }

    /// Creates an iterator over owned handles to the components of this path, starting at the `i`-th component. If `i` is greater than or equal to the number of components, the iterator yields zero items.
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn suffix_owned_components(
        &self,
        i: usize,
    ) -> impl DoubleEndedIterator<Item = OwnedComponent<MCL>>
           + ExactSizeIterator<Item = OwnedComponent<MCL>>
           + '_ {
        (i..self.component_count()).map(|i| {
            self.owned_component(i).unwrap() // Only `None` if `i >= self.component_count()`
        })
    }

    /// Creates a new path that consists of the first `component_count` components. More efficient than creating a new [`Path`] from scratch.
    ///
    /// Returns `None` if `component_count` is greater than `self.get_component_count()`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn create_prefix(&self, component_count: usize) -> Option<Self> {
        if component_count > self.component_count() {
            None
        } else {
            Some(unsafe { self.create_prefix_unchecked(component_count) })
        }
    }

    /// Creates a new path that consists of the first `component_count` components. More efficient than creating a new [`Path`] from scratch.
    ///
    /// #### Safety
    ///
    /// Undefined behaviour if `component_count` is greater than `self.component_count()`. May manifest directly, or at any later
    /// function invocation that operates on the resulting [`Path`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub unsafe fn create_prefix_unchecked(&self, component_count: usize) -> Self {
        Self {
            data: self.data.clone(),
            component_count,
        }
    }

    /// Returns the sum of the lengths of the first `component_count` components in this path. More efficient than `path.create_prefix(component_count).path_length()`.
    ///
    /// Guaranteed to be at most `MCC`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn path_length_of_prefix(&self, component_count: usize) -> usize {
        Representation::total_length(&self.data, component_count)
    }

    /// Creates an iterator over all prefixes of this path (including the empty path and the path itself).
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn all_prefixes(&self) -> impl DoubleEndedIterator<Item = Self> + '_ {
        (0..=self.component_count()).map(|i| {
            unsafe {
                self.create_prefix_unchecked(i) // safe to call for i <= self.component_count()
            }
        })
    }

    /// Tests whether this path is a prefix of the given path.
    /// Paths are always a prefix of themselves, and the empty path is a prefix of every path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the longer path in bytes, and `m` is the greatest number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return false;
            }
        }

        self.component_count() <= other.component_count()
    }

    /// Tests whether this path is prefixed by the given path.
    /// Paths are always a prefix of themselves.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the longer path in bytes, and `m` is the greatest number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn is_prefixed_by(&self, other: &Self) -> bool {
        other.is_prefix_of(self)
    }

    /// Tests whether this path is _related_ to the given path, that is, whether either one is a prefix of the other.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the longer path in bytes, and `m` is the greatest number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn is_related(&self, other: &Self) -> bool {
        self.is_prefix_of(other) || self.is_prefixed_by(other)
    }

    /// Returns the longest common prefix of this path and the given path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the shorter of the two paths, and `m` is the lesser number of components. Performs a single allocation of `O(n + m)` bytes to create the return value.
    pub fn longest_common_prefix(&self, other: &Self) -> Self {
        let mut lcp_len = 0;

        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                break;
            }

            lcp_len += 1
        }

        self.create_prefix(lcp_len).unwrap() // zip ensures that lcp_len <= self.component_count()
    }

    /// Returns the least path which is strictly greater than `self`, or return `None` if `self` is the greatest possible path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn successor(&self) -> Option<Self> {
        // If it is possible to append an empty component, then doing so yields the successor.
        if let Ok(path) = self.append(Component::new_empty()) {
            return Some(path);
        }

        // Otherwise, we try incrementing the final component. If that fails,
        // we try to increment the second-to-final component, and so on.
        // All components that come after the incremented component are discarded.
        // If *no* component can be incremented, `self` is the maximal path and we return `None`.

        for (i, component) in self.components().enumerate().rev() {
            // It would be nice to call a `try_increment_component` function, but in order to avoid
            // memory allocations, we write some lower-level but more efficient code.

            // If it is possible to append a zero byte to a component, then doing so yields its successor.
            if component.len() < MCL
                && Representation::sum_of_lengths_for_component(self.data.as_ref(), i) < MPL
            {
                // We now know how to construct the path successor of `self`:
                // Take the first `i` components (this *excludes* the current `component`),
                // then append `component` with an additional zero byte at the end.
                let mut buf = clone_prefix_and_lengthen_final_component(&self.data, i, 1);
                buf.put_u8(0);

                return Some(Path {
                    data: buf.freeze(),
                    component_count: i + 1,
                });
            }

            // We **cannot** append a zero byte, so instead we check whether we can treat the component as a fixed-width integer and increment it. The only failure case is if that component consists of 255-bytes only.
            let can_increment = !component.iter().all(|byte| *byte == 255);

            // If we cannot increment, we go to the next iteration of the loop. But if we can, we can create a copy of the
            // prefix on the first `i + 1` components, and mutate its backing memory in-place.
            if can_increment {
                let mut buf = clone_prefix_and_lengthen_final_component(&self.data, i, 0);

                let start_component_offset =
                    Representation::start_offset_of_component(buf.as_ref(), i);
                let end_component_offset = Representation::end_offset_of_component(buf.as_ref(), i);
                fixed_width_increment(
                    &mut buf.as_mut()[start_component_offset..end_component_offset],
                );

                return Some(Path {
                    data: buf.freeze(),
                    component_count: i + 1,
                });
            }
        }

        // Failed to increment any component, so `self` is the maximal path.
        None
    }

    /// Returns the least path that is strictly greater than `self` and which is not prefixed by `self`, or `None` if no such path exists.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    pub fn greater_but_not_prefixed(&self) -> Option<Self> {
        // We iterate through all components in reverse order. For each component, we check whether we can replace it by another cmponent that is strictly greater but not prefixed by the original component. If that is possible, we do replace it with the least such component and drop all later components. If that is impossible, we try again with the previous component. If this impossible for all components, then this function returns `None`.

        for (i, component) in self.components().enumerate().rev() {
            // If it is possible to append a zero byte to a component, then doing so yields its successor.
            if component.len() < MCL
                && Representation::sum_of_lengths_for_component(self.data.as_ref(), i) < MPL
            {
                let mut buf = clone_prefix_and_lengthen_final_component(&self.data, i, 1);
                buf.put_u8(0);

                return Some(Path {
                    data: buf.freeze(),
                    component_count: i + 1,
                });
            }

            // Next, we check whether the i-th component can be changed into the least component that is greater but not prefixed by the original. If so, do that and cut off all later components.
            let mut next_component_length = None;
            for (j, comp_byte) in component.iter().enumerate().rev() {
                if *comp_byte < 255 {
                    next_component_length = Some(j + 1);
                    break;
                }
            }

            if let Some(next_component_length) = next_component_length {
                // Yay, we can replace the i-th comopnent and then we are done.

                let mut buf = clone_prefix_and_lengthen_final_component(&self.data, i, 0);
                let length_of_prefix = Representation::sum_of_lengths_for_component(&buf, i);

                // Update the length of the final component.
                buf_set_final_component_length(
                    buf.as_mut(),
                    i,
                    length_of_prefix - (component.len() - next_component_length),
                );

                // Increment the byte at position `next_component_length` of the final component.
                let offset = Representation::start_offset_of_component(buf.as_ref(), i)
                    + next_component_length
                    - 1;
                let byte = buf.as_ref()[offset]; // guaranteed < 255...
                buf.as_mut()[offset] = byte + 1; // ... hence no overflow here.

                return Some(Path {
                    data: buf.freeze(),
                    component_count: i + 1,
                });
            }
        }

        None
    }

    /// Create a new path from a slice of byte slices.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n + m)`, where `n` is the total length of the path in bytes, and `m` is the number of components. Performs a single allocation of `O(n + m)` bytes.
    ///
    /// # Example
    ///
    /// ```
    /// # use willow_data_model::{Path, PathConstructionError, InvalidPathError};
    /// // Ok
    /// let path = Path::<12, 3, 30>::from_slices(&["alfie", "notes"]).unwrap();
    ///
    /// // Err
    /// let result1 = Path::<12, 3, 30>::from_slices(&["themaxpath", "lengthis30", "thisislonger"]);
    /// assert!(matches!(result1, Err(PathConstructionError::InvalidPath(InvalidPathError::PathTooLong))));
    ///
    /// // Err
    /// let result2 = Path::<12, 3, 30>::from_slices(&["too", "many", "components", "error"]);
    /// assert!(matches!(result2, Err(PathConstructionError::InvalidPath(InvalidPathError::TooManyComponents))));
    ///
    /// // Err
    /// let result3 = Path::<12, 3, 30>::from_slices(&["overencumbered"]);
    /// assert!(matches!(result3, Err(PathConstructionError::ComponentTooLongError)));
    /// ```
    pub fn from_slices<T: AsRef<[u8]>>(slices: &[T]) -> Result<Self, PathConstructionError> {
        let total_length = slices.iter().map(|it| it.as_ref().len()).sum();
        let mut builder = PathBuilder::new(total_length, slices.len())?;

        for component_slice in slices {
            let component = Component::<MCL>::new(component_slice.as_ref())
                .ok_or(PathConstructionError::ComponentTooLongError)?;
            builder.append_component(component);
        }

        Ok(builder.build())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialEq for Path<MCL, MCC, MPL> {
    fn eq(&self, other: &Self) -> bool {
        if self.component_count != other.component_count {
            false
        } else {
            self.components().eq(other.components())
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Eq for Path<MCL, MCC, MPL> {}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Hash for Path<MCL, MCC, MPL> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.component_count.hash(state);

        for comp in self.components() {
            comp.hash(state);
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialOrd for Path<MCL, MCC, MPL> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares paths lexicographically, since that is the path ordering that the Willow spec always uses.
impl<const MCL: usize, const MCC: usize, const MPL: usize> Ord for Path<MCL, MCC, MPL> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.components().cmp(other.components())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Debug for Path<MCL, MCC, MPL> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data_vec: Vec<_> = self.components().collect();

        f.debug_tuple("Path").field(&data_vec).finish()
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for Path<MCL, MCC, MPL>
{
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, ArbitraryError> {
        let mut total_length_in_bytes: usize = Arbitrary::arbitrary(u)?;
        total_length_in_bytes %= MPL + 1;

        let data: Box<[u8]> = Arbitrary::arbitrary(u)?;
        total_length_in_bytes = core::cmp::min(total_length_in_bytes, data.len());

        let mut num_components: usize = Arbitrary::arbitrary(u)?;
        num_components %= MCC + 1;

        if num_components == 0 {
            total_length_in_bytes = 0;
        }

        let mut builder = PathBuilder::new(total_length_in_bytes, num_components).unwrap();

        let mut length_total_so_far = 0;
        for i in 0..num_components {
            // Determine the length of the i-th component: randomly within some constraints for all but the final one. The final length is chosen to match the total_length_in_bytes.
            let length_of_ith_component = if i + 1 == num_components {
                if total_length_in_bytes - length_total_so_far > MCL {
                    return Err(ArbitraryError::IncorrectFormat);
                } else {
                    total_length_in_bytes - length_total_so_far
                }
            } else {
                // Any non-final component can take on a random length, ...
                let mut component_length: usize = Arbitrary::arbitrary(u)?;
                // ... except it must be at most the MCL, and...
                component_length %= MCL + 1;
                // ... the total length of all components must not exceed the total path length.
                component_length = core::cmp::min(
                    component_length,
                    total_length_in_bytes - length_total_so_far,
                );
                component_length
            };

            builder.append_component(
                Component::new(
                    &data[length_total_so_far..length_total_so_far + length_of_ith_component],
                )
                .unwrap(),
            );
            length_total_so_far += length_of_ith_component;
        }

        Ok(builder.build())
    }

    #[inline]
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        (
            and_all(&[
                usize::size_hint(depth),
                usize::size_hint(depth),
                Box::<[u8]>::size_hint(depth),
            ])
            .0,
            None,
        )
    }
}

/////////////////////////////////////////////////////////////////////
// Helpers for efficiently creating successors.                    //
// Efficiency warrants some low-level fiddling around here, sorry. //
/////////////////////////////////////////////////////////////////////

/// Creates a new BufMut that stores the heap encoding of the first i components of `original`, but increasing the length of the final component by `extra_capacity`. No data to fill that extra capacity is written into the buffer.
fn clone_prefix_and_lengthen_final_component(
    representation: &[u8],
    i: usize,
    extra_capacity: usize,
) -> BytesMut {
    let successor_path_length =
        Representation::sum_of_lengths_for_component(representation, i) + extra_capacity;
    let buf_capacity = size_of::<usize>() * (i + 2) + successor_path_length;
    let mut buf = BytesMut::with_capacity(buf_capacity);

    // Write the length of the successor path as the first usize.
    buf.extend_from_slice(&(i + 1).to_ne_bytes());

    // Next, copy the total path lengths for the first i prefixes.
    buf.extend_from_slice(&representation[size_of::<usize>()..size_of::<usize>() * (i + 2)]);

    // Now, write the length of the final component, which is one greater than before.
    buf_set_final_component_length(buf.as_mut(), i, successor_path_length);

    // Finally, copy the raw bytes of the first i+1 components.
    buf.extend_from_slice(
        &representation[Representation::start_offset_of_component(representation, 0)
            ..Representation::start_offset_of_component(representation, i + 1)],
    );

    buf
}

// In a buffer that stores a path on the heap, set the sum of all component lengths for the i-th component, which must be the final component.
fn buf_set_final_component_length(buf: &mut [u8], i: usize, new_sum_of_lengths: usize) {
    let comp_len_start = Representation::start_offset_of_sum_of_lengths_for_component(i);
    let comp_len_end = comp_len_start + size_of::<usize>();
    buf[comp_len_start..comp_len_end].copy_from_slice(&new_sum_of_lengths.to_ne_bytes()[..]);
}

// Overflows to all zeroes if all bytes are 255.
fn fixed_width_increment(buf: &mut [u8]) {
    for byte_ref in buf.iter_mut().rev() {
        if *byte_ref == 255 {
            *byte_ref = 0;
        } else {
            *byte_ref += 1;
            return;
        }
    }
}
