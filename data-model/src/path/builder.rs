use core::cmp::min;
use core::mem::size_of;

use bytes::{BufMut, BytesMut};
use either::Either::*;
use ufotofu::BulkProducer;
use ufotofu_codec::{Blame, DecodeError};

use super::{Component, InvalidPathError, Path, Representation};

/// A helper struct for creating a [`Path`] with exactly one memory allocation. Requires total length and component count to be known in advance.
///
/// Enforces that each component has a length of at most `MCL` ([**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length)), that each path has at most `MCC` ([**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count)) components, and that the total size in bytes of all components is at most `MPL` ([**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length)).
pub struct PathBuilder<const MCL: usize, const MCC: usize, const MPL: usize> {
    bytes: BytesMut,               // Turns into the [`HeapEncoding`] when building.
    initialised_components: usize, // How many of the components have been appended to the `bytes` already?
    target_length: usize,          // The total length that the path should have when building.
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PathBuilder<MCL, MCC, MPL> {
    /// Creates a builder for a path of known total length and component count.
    /// The component data must be filled in before building.
    pub fn new(total_length: usize, component_count: usize) -> Result<Self, InvalidPathError> {
        if total_length > MPL {
            return Err(InvalidPathError::PathTooLong);
        }

        if component_count > MCC {
            return Err(InvalidPathError::TooManyComponents);
        }

        // Allocate all storage in a single go.
        let mut buf = BytesMut::with_capacity(Representation::allocation_size(
            total_length,
            component_count,
        ));

        // Place the number of components at the start of the buffer.
        buf.extend_from_slice(&(component_count.to_ne_bytes())[..]);

        // Fill up the accumulated component lengths with dummy zeroes, so we can use buf.extend_from_slice to append the actual component data in `append_component`. We overwrite the zeroes with actual values in `append_component`.
        buf.put_bytes(0, component_count * size_of::<usize>());

        Ok(Self {
            bytes: buf,
            initialised_components: 0,
            target_length: total_length,
        })
    }

    /// Appends data for a component of known length by reading data from a [`BulkProducer`] of bytes. Panics if `component_length > MCL`.
    pub async fn append_component_from_bulk_producer<P>(
        &mut self,
        component_length: usize,
        p: &mut P,
    ) -> Result<(), DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
    {
        assert!(component_length <= MCL);

        // Compute the accumulated length for the new component.
        let total_length_so_far = match self.initialised_components.checked_sub(1) {
            Some(i) => Representation::sum_of_lengths_for_component(&self.bytes, i),
            None => 0,
        };
        let acc_length = component_length + total_length_so_far;

        // Overwrite the dummy accumulated component length for this component with the actual value.
        let start = Representation::start_offset_of_sum_of_lengths_for_component(
            self.initialised_components,
        );
        let end = start + size_of::<usize>();
        self.bytes.as_mut()[start..end].copy_from_slice(&acc_length.to_ne_bytes()[..]);

        // Ufotofu prohibits empty slices, so it is easier to handle the case that would require them explicitly.
        if component_length == 0 {
            // Record that we added a component.
            self.initialised_components += 1;
            return Ok(());
        }

        // Now, read bytes until we have the full component.
        let mut produced_so_far = 0;
        while produced_so_far < component_length {
            // Get as many bytes from the producer as efficiently possible.
            match p.expose_items().await? {
                Right(fin) => return Err(DecodeError::UnexpectedEndOfInput(fin)),
                Left(data) => {
                    let remaining_len = min(data.len(), component_length - produced_so_far);
                    self.bytes.extend_from_slice(&data[..remaining_len]);
                    p.consider_produced(remaining_len).await?;
                    produced_so_far += remaining_len;
                }
            }
        }

        // Record that we added a component.
        self.initialised_components += 1;

        Ok(())
    }

    /// Appends the data for the next component.
    pub fn append_component(&mut self, component: Component<MCL>) {
        // Compute the accumulated length for the new component.
        let total_length_so_far = match self.initialised_components.checked_sub(1) {
            Some(i) => Representation::sum_of_lengths_for_component(&self.bytes, i),
            None => 0,
        };
        let acc_length = component.len() + total_length_so_far;

        // Overwrite the dummy accumulated component length for this component with the actual value.
        let start = Representation::start_offset_of_sum_of_lengths_for_component(
            self.initialised_components,
        );
        let end = start + size_of::<usize>();
        self.bytes.as_mut()[start..end].copy_from_slice(&acc_length.to_ne_bytes()[..]);

        // Append the component to the path.
        self.bytes.extend_from_slice(component.as_ref());

        // Record that we added a component.
        self.initialised_components += 1;
    }

    /// Turn this builder into an immutable [`Path`].
    ///
    /// Panics if the number of components or the total length does not match what was claimed in [`PathBuilder::new`].
    pub fn build(self) -> Path<MCL, MCC, MPL> {
        // Check whether we appended the correct number of components.
        assert_eq!(
            self.initialised_components,
            Representation::component_count(self.bytes.as_ref())
        );

        assert_eq!(
            self.target_length,
            Representation::total_length(&self.bytes, self.initialised_components),
            "Expected a target length of {}, but got an actual total_length of {}\nRaw representation:{:?}",
            self.target_length,
            Representation::total_length(&self.bytes, self.initialised_components),
            self.bytes
        );

        Path {
            data: self.bytes.freeze(),
            component_count: self.initialised_components,
        }
    }
}
