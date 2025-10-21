use core::{borrow::Borrow, fmt, ops::Deref};

use willow_data_model::prelude as wdm;
use willow_data_model::prelude::InvalidComponentError;

use crate::MCL;

/// A slice of a [Path Component](https://willowprotocol.org/specs/data-model/index.html#Component).
///
/// This type statically enforces a maximal length of `4096` ([`MCL`]). Otherwise, it is basically a regular `[u8]`.
///
/// This is an *unsized* type, meaning that it must always be used behind a pointer like `&` or [`Box`].
///
/// Use the [`component!`](component) macro to create statically-known components, and the [`Component::new`], [`Component::new_empty`], and [`Component::new_unchecked`] methods for dynamically creating components.
///
/// ```
/// use willow25::prelude::*;
///
/// assert_eq!(component!("abc"), Component::new(b"abc").unwrap());
/// ```
///
/// Most of the time, you might be working with [`Paths`](crate::prelude::Path) and will have no need to directly interact with individual components.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Component(wdm::Component<MCL>);

impl Component {
    /// Creates a [`Component`] from a byte slice. Returns [`None`] if the slice is longer than 4096 ([`MCL`]).
    ///
    /// Aside from checking the length, this is a cost-free conversion.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow25::prelude::*;
    /// assert!(Component::new(b"yay").is_ok());
    /// assert!(Component::new(b"too_loooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong").is_err());
    /// ```
    pub fn new<'s>(s: &'s [u8]) -> Result<&'s Self, InvalidComponentError> {
        if s.len() <= MCL {
            Ok(unsafe { Self::new_unchecked(s) })
        } else {
            Err(InvalidComponentError)
        }
    }

    /// Creates a [`Component`] from a byte slice, without ensuring it consists of no more than 4096 ([`MCL`]) bytes.
    ///
    /// This is a cost-free conversion.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than 4096 ([`MCL`]) may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`Component`].
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow25::prelude::*;
    /// let unchecked_component = unsafe { Component::new_unchecked(b"yay") };
    /// assert_eq!(unchecked_component.as_bytes(), b"yay");
    /// ```
    pub unsafe fn new_unchecked<'s>(s: &'s [u8]) -> &'s Self {
        debug_assert!(s.len() <= MCL);
        unsafe { &*(s as *const [u8] as *const Self) }
    }

    /// Creates a `&'static` reference to the empty component.
    ///
    /// ```
    /// use willow25::prelude::*;
    ///
    /// assert_eq!(component!(""), Component::new_empty());
    /// ```
    pub fn new_empty() -> &'static Self {
        unsafe { Self::new_unchecked(&[]) }
    }

    /// Returns the raw bytes of the component.
    ///
    /// ```
    /// use willow25::prelude::*;
    /// assert_eq!(component!("yay").as_bytes(), b"yay");
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Deref for Component {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl AsRef<wdm::Component<MCL>> for Component {
    fn as_ref(&self) -> &wdm::Component<MCL> {
        &self.0
    }
}

impl Borrow<[u8]> for Component {
    fn borrow(&self) -> &[u8] {
        self.0.borrow()
    }
}

impl fmt::Debug for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// An owned [component](https://willowprotocol.org/specs/data-model/index.html#Component) of a Willow [Path](https://willowprotocol.org/specs/data-model/index.html#Path), using reference counting for cheap cloning. Typically obtained from a [`Path`](super::Path) instead of being created independently.
///
/// This type enforces a const-generic [maximum component length](https://willowprotocol.org/specs/data-model/index.html#max_component_length). Use the [`AsRef`], [`Deref`], or [`Borrow`] implementation to access the wrapped immutable byte slice.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OwnedComponent(pub(crate) wdm::OwnedComponent<MCL>);

impl OwnedComponent {
    /// Creates an [`OwnedComponent`] by copying data from a byte slice. Returns [`None`] if the slice is longer than 4096 ([`MCL`]).
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow25::prelude::*;
    /// assert!(OwnedComponent::new(b"yay").is_ok());
    /// assert!(OwnedComponent::new(b"too_loooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong").is_err());
    /// ```
    pub fn new(data: &[u8]) -> Result<Self, InvalidComponentError> {
        Ok(Self(wdm::OwnedComponent::new(data)?))
    }

    /// Creates an [`OwnedComponent`] by copying data from a byte slice, without verifying its length.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than 4096 ([`MCL`]) may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`OwnedComponent`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow25::prelude::*;
    /// let unchecked_component = unsafe { OwnedComponent::new_unchecked(b"yay") };
    /// assert_eq!(unchecked_component.as_bytes(), b"yay");
    /// ```
    pub unsafe fn new_unchecked(data: &[u8]) -> Self {
        Self(unsafe { wdm::OwnedComponent::new_unchecked(data) })
    }

    /// Returns an empty [`OwnedComponent`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(1)`, performs no allocations.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow25::prelude::*;
    /// let empty_component = OwnedComponent::new_empty();
    /// assert_eq!(empty_component.as_bytes(), &[]);
    /// assert_eq!(empty_component, OwnedComponent::default());
    /// ```
    pub fn new_empty() -> Self {
        Self(wdm::OwnedComponent::new_empty())
    }

    /// Returns the raw bytes of the component.
    ///
    /// ```
    /// use willow25::prelude::*;
    /// assert_eq!(OwnedComponent::new(b"yay").unwrap().as_bytes(), b"yay");
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl AsRef<wdm::OwnedComponent<MCL>> for OwnedComponent {
    fn as_ref(&self) -> &wdm::OwnedComponent<MCL> {
        &self.0
    }
}

impl From<wdm::OwnedComponent<MCL>> for OwnedComponent {
    fn from(value: wdm::OwnedComponent<MCL>) -> Self {
        Self(value)
    }
}

impl From<OwnedComponent> for wdm::OwnedComponent<MCL> {
    fn from(value: OwnedComponent) -> wdm::OwnedComponent<MCL> {
        value.0
    }
}

impl Deref for OwnedComponent {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl Borrow<[u8]> for OwnedComponent {
    fn borrow(&self) -> &[u8] {
        self.0.borrow()
    }
}

impl fmt::Debug for OwnedComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for OwnedComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
