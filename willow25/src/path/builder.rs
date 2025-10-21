use willow_data_model::prelude as wdm;

use crate::prelude::*;

/// A helper struct for creating a [`Path`] with exactly one memory allocation. Requires total length and component count to be known in advance.
///
/// Enforces that each [`Component`] has a length of at most 4096 ([`MCL`]), that each [`Path`] has at most 4096 ([`MCC`]) [`Component`]s, and that the total size in bytes of all [`Component`]s is at most 4096 ([`MPL`]).
///
/// ```
/// use willow25::prelude::*;
/// let mut builder = PathBuilder::new(4, 2)?;
/// builder.append_component(component!("hi"));
/// builder.append_slice(b"ho")?;
/// assert_eq!(builder.build(), path!("/hi/ho"));
/// # Ok::<(), PathError>(())
/// ```
pub struct PathBuilder(wdm::PathBuilder<MCL, MCC, MPL>);

impl PathBuilder {
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
    /// use willow25::prelude::*;
    /// let mut builder = PathBuilder::new(4, 2)?;
    /// builder.append_component(component!("hi"));
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), path!("/hi/ho"));
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn new(
        total_length: usize,
        component_count: usize,
    ) -> Result<Self, PathFromComponentsError> {
        Ok(Self(wdm::PathBuilder::new(total_length, component_count)?))
    }

    /// Creates a builder for a [`Path`] of known total length and component count, efficiently prefilled with the first `prefix_component_count` [`Component`]s of a given `reference` [`Path`]. Panics if there are not enough [`Component`]s in the `reference`.
    ///
    /// The missing component data must be filled in before building.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(target_total_length + target_component_count)`, performs a single allocation of `O(total_length + component_count)` bytes.
    ///
    /// ```
    /// use willow25::prelude::*;
    /// let p = path!("/hi/he");
    /// let mut builder = PathBuilder::new_from_prefix(4, 2, &p, 1)?;
    /// builder.append_component(component!("ho"));
    /// assert_eq!(builder.build(), path!("/hi/ho"));
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn new_from_prefix(
        target_total_length: usize,
        target_component_count: usize,
        reference: &Path,
        prefix_component_count: usize,
    ) -> Result<Self, PathFromComponentsError> {
        Ok(Self(wdm::PathBuilder::new_from_prefix(
            target_total_length,
            target_component_count,
            reference.as_ref(),
            prefix_component_count,
        )?))
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
    /// use willow25::prelude::*;
    /// let mut builder = PathBuilder::new(4, 2)?;
    /// builder.append_component(component!("hi"));
    /// builder.append_component(component!("ho"));
    /// assert_eq!(builder.build(), path!("/hi/ho"));
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn append_component(&mut self, component: &Component) {
        self.0.append_component(component.as_ref());
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
    /// use willow25::prelude::*;
    /// let mut builder = PathBuilder::new(4, 2)?;
    /// builder.append_slice(b"hi")?;
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), path!("/hi/ho"));
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn append_slice(&mut self, component: &[u8]) -> Result<(), InvalidComponentError> {
        self.0.append_slice(component)
    }

    /// Turns this builder into an immutable [`Path`].
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
    /// use willow25::prelude::*;
    /// let mut builder = PathBuilder::new(4, 2)?;
    /// builder.append_component(component!("hi"));
    /// builder.append_slice(b"ho")?;
    /// assert_eq!(builder.build(), path!("/hi/ho"));
    /// # Ok::<(), PathError>(())
    /// ```
    pub fn build(self) -> Path {
        Path(self.0.build())
    }
}
