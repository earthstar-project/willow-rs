use std::{borrow::Borrow, ops::Deref};

use bytes::Bytes;

/// A [component](https://willowprotocol.org/specs/data-model/index.html#Component) of a Willow Path.
///
/// This type is a thin wrapper around `&'a [u8]` that enforces a const-generic [maximum component length](https://willowprotocol.org/specs/data-model/index.html#max_component_length). Use the `AsRef`, `DeRef`, or `Borrow` implementation to access the immutable byte slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Component<'a, const MAX_COMPONENT_LENGTH: usize>(&'a [u8]);

impl<'a, const MAX_COMPONENT_LENGTH: usize> Component<'a, MAX_COMPONENT_LENGTH> {
    /// Creates a `Component` from a byte slice. Return `None` if the slice is longer than `MaxComponentLength`.
    pub fn new(slice: &'a [u8]) -> Option<Self> {
        if slice.len() <= MAX_COMPONENT_LENGTH {
            return Some(unsafe { Self::new_unchecked(slice) }); // Safe because we just checked the length.
        } else {
            None
        }
    }

    /// Creates a `Component` from a byte slice, without verifying its length.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than `MAX_COMPONENT_LENGTH` may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`Component`].
    pub unsafe fn new_unchecked(slice: &'a [u8]) -> Self {
        Self(slice)
    }

    /// Returns an empty component.
    pub fn new_empty() -> Self {
        Self(&[])
    }

    /// Consumes self and returns the raw underlying slice.
    pub fn into_inner(self) -> &'a [u8] {
        self.0
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> Deref for Component<'a, MAX_COMPONENT_LENGTH> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> AsRef<[u8]> for Component<'a, MAX_COMPONENT_LENGTH> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl<'a, const MAX_COMPONENT_LENGTH: usize> Borrow<[u8]> for Component<'a, MAX_COMPONENT_LENGTH> {
    fn borrow(&self) -> &[u8] {
        self.0
    }
}

/// An owned [component](https://willowprotocol.org/specs/data-model/index.html#Component) of a Willow Path that uses reference counting for cheap cloning.
///
/// This type enforces a const-generic [maximum component length](https://willowprotocol.org/specs/data-model/index.html#max_component_length). Use the `AsRef`, `DeRef`, or `Borrow` implementation to access the immutable byte slice.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnedComponent<const MAX_COMPONENT_LENGTH: usize>(pub(crate) Bytes);

impl<const MAX_COMPONENT_LENGTH: usize> OwnedComponent<MAX_COMPONENT_LENGTH> {
    /// Creates an `OwnedComponent` by copying data from a byte slice. Return `None` if the slice is longer than `MaxComponentLength`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    pub fn new(data: &[u8]) -> Option<Self> {
        if data.len() <= MAX_COMPONENT_LENGTH {
            Some(unsafe { Self::new_unchecked(data) }) // Safe because we just checked the length.
        } else {
            None
        }
    }

    /// Creates an `OwnedComponent` by copying data from a byte slice, without verifying its length.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than `MAX_COMPONENT_LENGTH` may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`OwnedComponent`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    pub unsafe fn new_unchecked(data: &[u8]) -> Self {
        Self(Bytes::copy_from_slice(data))
    }

    /// Returns an empty component.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    pub fn new_empty() -> Self {
        Self(Bytes::new())
    }
}

impl<const MAX_COMPONENT_LENGTH: usize> Deref for OwnedComponent<MAX_COMPONENT_LENGTH> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<const MAX_COMPONENT_LENGTH: usize> AsRef<[u8]> for OwnedComponent<MAX_COMPONENT_LENGTH> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<const MAX_COMPONENT_LENGTH: usize> Borrow<[u8]> for OwnedComponent<MAX_COMPONENT_LENGTH> {
    fn borrow(&self) -> &[u8] {
        self.0.borrow()
    }
}
