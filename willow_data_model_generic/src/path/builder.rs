use core::mem::size_of;

use bytes::{BufMut, BytesMut};

use crate::prelude::*;

use super::Representation;

/// A helper struct for creating a [`Path`] with exactly one memory allocation. Requires total length and component count to be known in advance.
///
/// Enforces that each [`Component`] has a length of at most `MCL` ([**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length)), that each [`Path`] has at most `MCC` ([**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count)) [`Component`]s, and that the total size in bytes of all [`Component`]s is at most `MPL` ([**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length)).
///
/// ```
/// use willow_data_model_generic::prelude::*;
/// let mut builder = PathBuilder::<4, 4, 4>::new(4, 2)?;
/// builder.append_component(Component::new(b"hi")?);
/// builder.append_slice(b"ho")?;
/// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
/// # Ok::<(), PathError>(())
/// ```
pub struct PathBuilder<const MCL: usize, const MCC: usize, const MPL: usize> {
    bytes: BytesMut,               // Turns into the [`HeapEncoding`] when building.
    initialised_components: usize, // How many of the components have been appended to the `bytes` already?
    target_length: usize,          // The total length that the path should have when building.
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PathBuilder<MCL, MCC, MPL> {
    /// Creates a builder for a [`Path`] of known total length and component count.
    /// The component data must be filled in before building.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(total_length + component_count)`, performs a single allocation of `O(total_length + component_count)` bytes.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let mut builder = PathBuilder::<4, 4, 4>::new(4, 2)?;
    /// builder.append_component(Component::new(b"hi")?);
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn new(
        total_length: usize,
        component_count: usize,
    ) -> Result<Self, PathFromComponentsError> {
        if total_length > MPL {
            return Err(PathFromComponentsError::PathTooLong);
        }

        if component_count > MCC {
            return Err(PathFromComponentsError::TooManyComponents);
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

    /// Creates a builder for a [`Path`] of known total length and component count, efficiently prefilled with the first `prefix_component_count` [`Component`]s of a given `reference` [`Path`]. Panics if there are not enough [`Component`]s in the `reference`.
    /// The missing component data must be filled in before building.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(target_total_length + target_component_count)`, performs a single allocation of `O(total_length + component_count)` bytes.
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let p = Path::<4, 4, 4>::from_slices(&[b"hi", b"he"])?;
    /// let mut builder = PathBuilder::new_from_prefix(4, 2, &p, 1)?;
    /// builder.append_component(Component::new(b"ho")?);
    /// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn new_from_prefix(
        target_total_length: usize,
        target_component_count: usize,
        reference: &Path<MCL, MCC, MPL>,
        prefix_component_count: usize,
    ) -> Result<Self, PathFromComponentsError> {
        let mut builder = Self::new(target_total_length, target_component_count)?;

        assert!(reference.component_count() >= prefix_component_count);

        // The following is logically equivalent to successively appending the first `prefix_component_count` components of `reference` to the builder, but more efficient in terms of `memcpy` calls.
        match prefix_component_count.checked_sub(1) {
            None => return Ok(builder),
            Some(index_of_final_component) => {
                // Overwrite the dummy accumulated component lengths for the first `prefix_component_count` components with the actual values.
                let accumulated_component_lengths_start =
                    Representation::start_offset_of_sum_of_lengths_for_component(0);
                let accumulated_component_lengths_end =
                    Representation::start_offset_of_sum_of_lengths_for_component(
                        index_of_final_component + 1,
                    );
                builder.bytes.as_mut()
                    [accumulated_component_lengths_start..accumulated_component_lengths_end]
                    .copy_from_slice(
                        &reference.data[accumulated_component_lengths_start
                            ..accumulated_component_lengths_end],
                    );

                let components_start_in_prefix =
                    Representation::start_offset_of_component(&reference.data, 0);
                let components_end_in_prefix = Representation::end_offset_of_component(
                    &reference.data,
                    index_of_final_component,
                );
                builder.bytes.extend_from_slice(
                    &reference.data[components_start_in_prefix..components_end_in_prefix],
                );

                // Record that we added the components.
                builder.initialised_components = prefix_component_count;
            }
        }

        // For reference, here is a naive implementation of the same functionality:

        // for comp in reference
        //     .create_prefix(prefix_component_count)
        //     .expect("`prefix_component_count` must be less than the number of components in `reference`")
        //     .components()
        // {
        //     builder.append_component(comp);
        // }

        Ok(builder)
    }

    /// Appends the data for the next [`Component`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(component_length)` time. Performs no allocations.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let mut builder = PathBuilder::<4, 4, 4>::new(4, 2)?;
    /// builder.append_component(Component::new(b"hi")?);
    /// builder.append_component(Component::new(b"ho")?);
    /// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn append_component(&mut self, component: &Component<MCL>) {
        // Compute the accumulated length for the new component.
        let total_length_so_far = match self.initialised_components.checked_sub(1) {
            Some(i) => Representation::sum_of_lengths_for_component(&self.bytes, i),
            None => 0,
        };
        let acc_length = component.as_ref().len() + total_length_so_far;

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

    /// Appends the data for the next [`Component`], from a slice of bytes.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(component_length)` time. Performs no allocations.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let mut builder = PathBuilder::<4, 4, 4>::new(4, 2)?;
    /// builder.append_slice(b"hi")?;
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn append_slice<T: AsRef<[u8]>>(
        &mut self,
        component: T,
    ) -> Result<(), InvalidComponentError> {
        Ok(self.append_component(Component::new(component.as_ref())?))
    }

    /// Turn this builder into an immutable [`Path`].
    ///
    /// Panics if the number of [`Component`]s or the total length does not match what was claimed in [`PathBuilder::new`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)` time. Performs no allocations.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let mut builder = PathBuilder::<4, 4, 4>::new(4, 2)?;
    /// builder.append_component(Component::new(b"hi")?);
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), Path::from_slices(&[b"hi", b"ho"])?);
    /// # Ok::<(), PathError>(())
    /// ```
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
