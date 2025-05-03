use core::mem::size_of;

use super::Component;

/// The way we represent a path as a contiguous slice of bytes in memory.
///
/// - First, a usize that gives the total number of path components.
/// - Second, that many usizes, where the i-th one gives the sum of the lengths of the first i components.
/// - Third, the concatenation of all components.
///
/// Note that these are not guaranteed to fulfil alignment requirements of usize, so we need to be careful in how we access these.
/// Always use the methods on this struct for that reason.
///
/// This is a struct only for namespacing, everything operates on `&[u8]`s.
pub(crate) struct Representation;

impl Representation {
    /// Gets the number of components.
    pub fn component_count(buf: &[u8]) -> usize {
        Self::usize_at_offset(buf, 0)
    }

    /// Gets the total length of the path in bytes, but only considering its `component_count` many first components.
    pub fn total_length(buf: &[u8], component_count: usize) -> usize {
        match component_count.checked_sub(1) {
            None => 0,
            Some(i) => Self::sum_of_lengths_for_component(buf, i),
        }
    }

    #[allow(dead_code)] // Because this will come in handy one day.
    /// Gets the length of the i`-th component.
    ///
    /// Panics if `i` is outside the slice.
    pub fn component_len(buf: &[u8], i: usize) -> usize {
        if i == 0 {
            Self::sum_of_lengths_for_component(buf, i)
        } else {
            Self::sum_of_lengths_for_component(buf, i)
                - Self::sum_of_lengths_for_component(buf, i - 1)
        }
    }

    /// Gets the `i`-th component.
    ///
    /// Panics if `i` is outside the slice.
    pub fn component<const MCL: usize>(buf: &[u8], i: usize) -> Component<MCL> {
        let start = Self::start_offset_of_component(buf, i);
        let end = Self::end_offset_of_component(buf, i);
        Component::new(&buf[start..end]).unwrap()
    }

    /// Gets the sum of the lengths of the first `i` components.
    ///
    /// Panics if `i` is outside the slice.
    pub fn sum_of_lengths_for_component(buf: &[u8], i: usize) -> usize {
        let start_offset_in_bytes = Self::start_offset_of_sum_of_lengths_for_component(i);
        Self::usize_at_offset(buf, start_offset_in_bytes)
    }

    /// Gets how many bytes are required for a path representation of a given `total_length` and `component_count`.
    pub fn allocation_size(total_length: usize, component_count: usize) -> usize {
        total_length + ((component_count + 1) * size_of::<usize>())
    }

    /// Gets the offset in bytes where the accumulated sum of lengths of the first `i` components is stored.
    pub fn start_offset_of_sum_of_lengths_for_component(i: usize) -> usize {
        // First usize is the number of components, then the i usizes storing the lengths; hence at i + 1.
        size_of::<usize>() * (i + 1)
    }

    /// Gets the offset in bytes at which the `i`-th component starts. Panics if `i` is outside the slice.
    pub fn start_offset_of_component(buf: &[u8], i: usize) -> usize {
        let metadata_length = (Self::component_count(buf) + 1) * size_of::<usize>();
        if i == 0 {
            metadata_length
        } else {
            Self::sum_of_lengths_for_component(buf, i - 1) // Length of everything up until the previous component.
            + metadata_length
        }
    }

    /// Gets the offset in bytes at which the `i`-th component ends (exclusive). Panics if `i` is outside the slice.
    pub fn end_offset_of_component(buf: &[u8], i: usize) -> usize {
        let metadata_length = (Self::component_count(buf) + 1) * size_of::<usize>();
        Self::sum_of_lengths_for_component(buf, i) + metadata_length
    }

    fn usize_at_offset(buf: &[u8], offset_in_bytes: usize) -> usize {
        let end = offset_in_bytes + size_of::<usize>();

        // We cannot interpret the memory in the slice as a usize directly, because the alignment might not match.
        // So we first copy the bytes onto the heap, then construct a usize from it.
        let mut usize_bytes = [0u8; size_of::<usize>()];

        if buf.len() < end {
            panic!();
        } else {
            usize_bytes.copy_from_slice(&buf[offset_in_bytes..end]);
            usize::from_ne_bytes(usize_bytes)
        }
    }
}
