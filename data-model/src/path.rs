// This struct is tested in `fuzz/path.rs`, `fuzz/path2.rs`, `fuzz/path3.rs`.
// Further, successors and prefix_successors are tested in `fuzz/path_successor.rs` and friends, and `fuzz/path_successor_of_prefix.rs` and friends.

use core::borrow::Borrow;
use core::convert::AsRef;
use core::fmt::Debug;
use core::hash::Hash;
use core::iter;
use core::mem::size_of;
use core::ops::Deref;

use bytes::{BufMut, Bytes, BytesMut};

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

    /// Create an empty component.
    pub fn new_empty() -> Self {
        return Self(&[]);
    }

    pub fn into_inner(self) -> &'a [u8] {
        return self.0;
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

/// An immutable Willow [path](https://willowprotocol.org/specs/data-model/index.html#Path). Thread-safe, cheap to clone, cheap to take prefixes of, expensive to append to.
///
/// Enforces that each component has a length of at most `MCL` ([**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length)), that each path has at most `MCC` ([**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count)) components, and that the total size in bytes of all components is at most `MPL` ([**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length)).
#[derive(Clone)]
pub struct Path<const MCL: usize, const MCC: usize, const MPL: usize> {
    /// The data of the underlying path.
    data: HeapEncoding<MCL>,
    /// Number of components of the `data` to consider for this particular path. Must be less than or equal to the total number of components.
    component_count: usize,
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
            component_count: 0,
        };
    }

    /// Construct a singleton path, i.e., a path of exactly one component.
    ///
    /// Copies the bytes of the component into an owned allocation on the heap.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the component. Performs a single allocation of `O(n)` bytes.
    pub fn new_singleton<'a>(comp: Component<'a, MCL>) -> Result<Self, InvalidPathError> {
        if 1 > MCC {
            return Err(InvalidPathError::TooManyComponents);
        } else if comp.len() > MPL {
            return Err(InvalidPathError::PathTooLong);
        } else {
            let mut buf = BytesMut::with_capacity((2 * size_of::<usize>()) + comp.len());
            buf.extend_from_slice(&(1usize.to_ne_bytes())[..]);
            buf.extend_from_slice(&(comp.len().to_ne_bytes())[..]);
            buf.extend_from_slice(comp.as_ref());

            return Ok(Path {
                data: HeapEncoding(buf.freeze()),
                component_count: 1,
            });
        }
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
        buf.put_bytes(0, component_count * size_of::<usize>());

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
            component_count,
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
        return self.component_count;
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
        if self.component_count == 0 {
            return 0;
        } else {
            return HeapEncoding::<MCL>::get_sum_of_lengths_for_component(
                self.data.as_ref(),
                self.component_count - 1,
            )
            .unwrap();
        }
    }

    /// Get the `i`-th [`Component`] of this path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn get_component<'s>(&'s self, i: usize) -> Option<Component<'s, MCL>> {
        return HeapEncoding::<MCL>::get_component(self.data.as_ref(), i);
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

    /// Create a new path that consists of the first `length` components. More efficient than creating a new [`Path`] from scratch.
    ///
    /// Returns `None` if `length` is greater than `self.get_component_count()`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn create_prefix(&self, length: usize) -> Option<Self> {
        if length > self.get_component_count() {
            return None;
        } else {
            return Some(unsafe { self.create_prefix_unchecked(length) });
        }
    }

    /// Create a new path that consists of the first `length` components. More efficient than creating a new [`Path`] from scratch.
    ///
    /// Undefined behaviour if `length` is greater than `self.get_component_count()`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub unsafe fn create_prefix_unchecked(&self, length: usize) -> Self {
        Self {
            data: self.data.clone(),
            component_count: length,
        }
    }

    /// Create an iterator over all prefixes of this path (including th empty path and the path itself).
    ///
    /// Stepping the iterator takes `O(1)` time and performs no memory allocations.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn all_prefixes(&self) -> impl DoubleEndedIterator<Item = Self> + '_ {
        (0..=self.get_component_count()).map(|i| {
            unsafe {
                self.create_prefix_unchecked(i) // safe to call for i <= self.get_component_count()
            }
        })
    }

    /// Test whether this path is a prefix of the given path.
    /// Paths are always a prefix of themselves, and the empty path is a prefix of every path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the shorter of the two paths. Performs no allocations.
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return false;
            }
        }

        self.get_component_count() <= other.get_component_count()
    }

    /// Test whether this path is prefixed by the given path.
    /// Paths are always a prefix of themselves.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the shorter of the two paths. Performs no allocations.
    pub fn is_prefixed_by(&self, other: &Self) -> bool {
        other.is_prefix_of(self)
    }

    /// Return the longest common prefix of this path and the given path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the shorter of the two paths. Performs a single allocation to create the return value.
    pub fn longest_common_prefix(&self, other: &Self) -> Self {
        let mut lcp_len = 0;

        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                break;
            }

            lcp_len += 1
        }

        self.create_prefix(lcp_len).unwrap() // zip ensures that lcp_len <= self.get_component_count()
    }

    /// Return the least path which is strictly greater than `self`, or return `None` if `self` is the greatest possible path.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the total length of the shorter of the two paths. Performs a single allocation to create the return value.
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
                && HeapEncoding::<MCL>::get_sum_of_lengths_for_component(self.data.as_ref(), i)
                    .unwrap() // i < self.component_count
                    < MPL
            {
                // We now know how to construct the path successor of `self`:
                // Take the first `i` components (this *excludes* the current `component`),
                // then append `component` with an additinoal zero byte at the end.

                // To minimise allocations, we implement this construction explicitly rather than using
                // our general-purpose functions. In particular, there is no need to explicitly
                // construct `component + zero_byte` on the heap.

                // First, we compute the total length of the allocation we need, then allocate it.
                let successor_path_length =
                    HeapEncoding::<MCL>::get_sum_of_lengths_for_component(self.data.as_ref(), i)
                        .unwrap()
                        + 1; // Can unwrap because `i < self.component_count`.
                let buf_capacity = size_of::<usize>() * (i + 2) + successor_path_length;
                let mut buf = BytesMut::with_capacity(buf_capacity);

                // Write the length of the successor path as the first usize.
                buf.extend_from_slice(&((i + 1) as usize).to_ne_bytes());

                // Next, copy the total path lengths for the first i prefixes.
                buf.extend_from_slice(
                    &self.data.0[size_of::<usize>()..size_of::<usize>() * (i + 2)],
                );

                // Now, write the length of the final component, which is one greater than before.
                buf_set_final_component_length(buf.as_mut(), i, successor_path_length);

                // Finally, copy the raw bytes of the first i+1 components...
                buf.extend_from_slice(
                    &self.data.0[HeapEncoding::<MCL>::start_offset_of_component(
                        &self.data.as_ref(),
                        0,
                    )
                    .unwrap()
                        ..HeapEncoding::<MCL>::start_offset_of_component(
                            &self.data.as_ref(),
                            i + 1,
                        )
                        .unwrap()],
                );

                // ... and append a zero byte.
                buf.put_u8(0);

                return Some(Self::from_buffer_and_component_count(buf.freeze(), i + 1));
            }

            // We **cannot** append a zero byte, so instead we check whether we can treat the component as a fixed-width integer and increment it. The only failure case is if that component consists of 255-bytes only.
            let can_increment = !component.iter().all(|byte| *byte == 255);

            // If we cannot increment, we go to the next iteration of the loop. But if we can, we can create a copy of the
            // prefix on the first `i + 1` components, and mutate its backing memory in-place.
            if can_increment {
                // First, we compute the total length of the allocation we need, then allocate it.
                let successor_path_length =
                    HeapEncoding::<MCL>::get_sum_of_lengths_for_component(self.data.as_ref(), i)
                        .unwrap(); // Can unwrap because `i < self.component_count`.
                let buf_capacity = size_of::<usize>() * (i + 2) + successor_path_length;
                let mut buf = BytesMut::with_capacity(buf_capacity);

                // Write the length of the successor path as the first usize.
                buf.extend_from_slice(&((i + 1) as usize).to_ne_bytes());

                // Next, copy the total path lengths for the first i+1 prefixes.
                buf.extend_from_slice(
                    &self.data.0[size_of::<usize>()..size_of::<usize>() * (i + 2)],
                );

                // Finally, copy the raw bytes of the first i+1 components.
                buf.extend_from_slice(
                    &self.data.0[HeapEncoding::<MCL>::start_offset_of_component(
                        &self.data.as_ref(),
                        0,
                    )
                    .unwrap()
                        ..HeapEncoding::<MCL>::start_offset_of_component(
                            &self.data.as_ref(),
                            i + 1,
                        )
                        .unwrap()],
                );

                let start_component_offset =
                    HeapEncoding::<MCL>::start_offset_of_component(buf.as_ref(), i).unwrap(); // i < self.component_count
                let end_component_offset =
                    HeapEncoding::<MCL>::end_offset_of_component(buf.as_ref(), i).unwrap(); // i < self.component_count
                fixed_width_increment(
                    &mut buf.as_mut()[start_component_offset..end_component_offset],
                );

                return Some(Self::from_buffer_and_component_count(buf.freeze(), i + 1));
            }
        }

        // Failed to increment any component, so `self` is the maximal path.
        None
    }

    fn from_buffer_and_component_count(buf: Bytes, component_count: usize) -> Self {
        Path {
            data: HeapEncoding(buf),
            component_count,
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialEq for Path<MCL, MCC, MPL> {
    fn eq(&self, other: &Self) -> bool {
        if self.component_count != other.component_count {
            return false;
        } else {
            return self.components().eq(other.components());
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

/// Compare paths lexicogrphically, since that is the path ordering that the Willow spec always uses.
impl<const MCL: usize, const MCC: usize, const MPL: usize> Ord for Path<MCL, MCC, MPL> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        return self.components().cmp(other.components());
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Debug for Path<MCL, MCC, MPL> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data_vec: Vec<_> = self.components().collect();

        f.debug_tuple("Path").field(&data_vec).finish()
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
#[derive(Clone)]
struct HeapEncoding<const MAX_COMPONENT_LENGTH: usize>(Bytes);

// All offsets are in bytes, unless otherwise specified.
// Arguments named `i` are the index of a component in the string, *not* a byte offset.
impl<const MAX_COMPONENT_LENGTH: usize> HeapEncoding<MAX_COMPONENT_LENGTH> {
    fn get_component_count(buf: &[u8]) -> usize {
        Self::get_usize_at_offset(buf, 0).unwrap() // We always store at least 8byte for the component count.
    }

    // None if i is outside the slice.
    fn get_sum_of_lengths_for_component(buf: &[u8], i: usize) -> Option<usize> {
        let start_offset_in_bytes = Self::start_offset_of_sum_of_lengths_for_component(i);
        Self::get_usize_at_offset(buf, start_offset_in_bytes)
    }

    fn get_component<'s>(buf: &'s [u8], i: usize) -> Option<Component<'s, MAX_COMPONENT_LENGTH>> {
        let start = Self::start_offset_of_component(buf, i)?;
        let end = Self::end_offset_of_component(buf, i)?;
        return Some(unsafe { Component::new_unchecked(&buf[start..end]) });
    }

    fn start_offset_of_sum_of_lengths_for_component(i: usize) -> usize {
        // First usize is the number of components, then the i usizes storing the lengths; hence at i + 1.
        size_of::<usize>() * (i + 1)
    }

    fn start_offset_of_component(buf: &[u8], i: usize) -> Option<usize> {
        let metadata_length = (Self::get_component_count(buf) + 1) * size_of::<usize>();
        if i == 0 {
            return Some(metadata_length);
        } else {
            return Self::get_sum_of_lengths_for_component(buf, i - 1) // Length of everything up until the previous component.
                .map(|length| length + metadata_length);
        }
    }

    fn end_offset_of_component(buf: &[u8], i: usize) -> Option<usize> {
        let metadata_length = (Self::get_component_count(buf) + 1) * size_of::<usize>();
        return Self::get_sum_of_lengths_for_component(buf, i)
            .map(|length| length + metadata_length);
    }

    fn get_usize_at_offset(buf: &[u8], offset_in_bytes: usize) -> Option<usize> {
        let end = offset_in_bytes + size_of::<usize>();

        // We cannot interpret the memory in the slice as a usize directly, because the alignment might not match.
        // So we first copy the bytes onto the heap, then construct a usize from it.
        let mut usize_bytes = [0u8; size_of::<usize>()];

        if buf.len() < end {
            return None;
        } else {
            usize_bytes.copy_from_slice(&buf[offset_in_bytes..end]);
            return Some(usize::from_ne_bytes(usize_bytes));
        }
    }
}

impl<const MAX_COMPONENT_LENGTH: usize> AsRef<[u8]> for HeapEncoding<MAX_COMPONENT_LENGTH> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

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

// In a buffer that stores a path on the heap, set the sum of all component lengths for the i-th component, which must be the final component.
fn buf_set_final_component_length(buf: &mut [u8], i: usize, new_sum_of_lengths: usize) {
    let comp_len_start = (1 + i) * size_of::<usize>();
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
