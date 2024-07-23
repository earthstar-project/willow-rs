//! Shared buffers to be reused across many decoding operations.

use core::mem::MaybeUninit;

use bytes::BytesMut;

use crate::path::Path;

/// A memory region to use for decoding paths. Reused between many decodings.
#[derive(Debug)]
pub(crate) struct ScratchSpacePathDecoding<const MCC: usize, const MPL: usize> {
    // The i-th usize holds the total lengths of the first i components.
    component_accumulated_lengths: [MaybeUninit<usize>; MCC],
    path_data: [MaybeUninit<u8>; MPL],
}

impl<const MCC: usize, const MPL: usize> ScratchSpacePathDecoding<MCC, MPL> {
    pub fn new() -> Self {
        ScratchSpacePathDecoding {
            component_accumulated_lengths: MaybeUninit::uninit_array(),
            path_data: MaybeUninit::uninit_array(),
        }
    }

    /// Panic if i >= MCC.
    pub fn set_component_accumulated_length(
        &mut self,
        component_accumulated_length: usize,
        i: usize,
    ) {
        MaybeUninit::write(
            &mut self.component_accumulated_lengths[i],
            component_accumulated_length,
        );
    }

    /// # Saftey
    /// 
    /// UB if length of slice is greater than `size_of::<usize>() * MCC`.
    pub unsafe fn set_many_component_accumulated_lengths_from_ne(
        &mut self,
        lengths: &[u8],
    ) {
        let slice: &mut [MaybeUninit<u8>] = core::slice::from_raw_parts_mut(
            self.component_accumulated_lengths[..lengths.len() / size_of::<usize>()].as_mut_ptr() as *mut MaybeUninit<u8>,
            lengths.len(),
        );
        MaybeUninit::copy_from_slice(
            slice,
            lengths
        );
    }

    /// # Safety
    ///
    /// Memory must have been initialised with a prior call to set_component_accumulated_length for the same `i`.
    unsafe fn get_component_accumulated_length(&self, i: usize) -> usize {
        MaybeUninit::assume_init(self.component_accumulated_lengths[i])
    }

    /// Return a slice of the accumulated component lengths up to but excluding the `i`-th component, encoded as native-endian u8s.
    ///
    /// # Safety
    ///
    /// Memory must have been initialised with prior call to set_component_accumulated_length for all `j <= i`
    pub unsafe fn get_accumumulated_component_lengths(&self, i: usize) -> &[u8] {
        core::slice::from_raw_parts(
            MaybeUninit::slice_assume_init_ref(&self.component_accumulated_lengths[..i]).as_ptr()
                as *const u8,
            i * size_of::<usize>(),
        )
    }

    /// Return a mutable slice of the i-th path_data.
    ///
    /// # Safety
    ///
    /// Accumulated component lengths for `i` and `i - 1` must have been set (only for `i` if `i == 0`).
    pub unsafe fn path_data_as_mut(&mut self, i: usize) -> &mut [MaybeUninit<u8>] {
        let start = if i == 0 {
            0
        } else {
            self.get_component_accumulated_length(i - 1)
        };
        let end = self.get_component_accumulated_length(i);
        &mut self.path_data[start..end]
    }

    /// Return a mutable slice of the path_data up to but excluding the i-th component.
    ///
    /// # Safety
    ///
    /// Accumulated component lengths for `i - 1` must have been set (unless `i == 0`).
    pub unsafe fn path_data_until_as_mut(&mut self, i: usize) -> &mut [MaybeUninit<u8>] {
        let end = self.get_component_accumulated_length(i - 1);
        &mut self.path_data[0..end]
    }

    /// Get the path data of the first `i` components.
    ///
    /// # Safety
    ///
    /// Memory must have been initialised with a prior call to set_component_accumulated_length for `i - 1` (ignored if `i == 0`).
    /// Also, the path data must have been initialised via a reference obtained from `self.path_data_as_mut()`.
    unsafe fn get_path_data(&self, i: usize) -> &[u8] {
        let end = if i == 0 {
            0
        } else {
            self.get_component_accumulated_length(i - 1)
        };
        return MaybeUninit::slice_assume_init_ref(&self.path_data[..end]);
    }

    /// Copy the data from this struct into a new Path of `i` components.
    ///
    /// # Safety
    ///
    /// The first `i` accumulated component lengths must have been set, and all corresponding path data must be initialised. MCL, MCC, and MCP are trusted blindly and must be adhered to by the data in the scratch buffer.
    pub unsafe fn to_path<const MCL: usize>(&self, i: usize) -> Path<MCL, MCC, MPL> {
        if i == 0 {
            Path::new_empty()
        } else {
            let total_length = if i == 0 {
                0
            } else {
                self.get_component_accumulated_length(i - 1)
            };
            let mut buf = BytesMut::with_capacity((size_of::<usize>() * (i + 1)) + total_length);

            buf.extend_from_slice(&i.to_ne_bytes());
            buf.extend_from_slice(self.get_accumumulated_component_lengths(i));
            buf.extend_from_slice(self.get_path_data(i));

            Path::from_buffer_and_component_count(buf.freeze(), i)
        }
    }
}
