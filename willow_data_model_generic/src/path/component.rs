use core::{borrow::Borrow, fmt, ops::Deref};
use std::fmt::{Debug, Write};

use bytes::Bytes;

/// A slice of a [Path Component](https://willowprotocol.org/specs/data-model/index.html#Component).
///
/// This type statically enforces a [**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length) of `MCL`. Otherwise, it is basically a regular `[u8]`.
///
/// This is an *unsized* type, meaning that it must always be used behind a pointer like `&` or [`Box`].
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Component<const MCL: usize>([u8]);

impl<const MCL: usize> Component<MCL> {
    /// Creates a [`Component`] from a byte slice. Returns [`None`] if the slice is longer than `MCL`.
    ///
    /// Aside from checking the length, this is a cost-free conversion.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// assert!(Component::<3>::new(b"yay").is_ok());
    /// assert!(Component::<3>::new(b"too_long").is_err());
    /// ```
    pub fn new<S: AsRef<[u8]> + ?Sized>(s: &S) -> Result<&Self, InvalidComponentError> {
        if s.as_ref().len() <= MCL {
            Ok(unsafe { Self::new_unchecked(s) })
        } else {
            Err(InvalidComponentError)
        }
    }

    /// Creates a `&'static` reference to the empty component.
    pub fn new_empty() -> &'static Self {
        unsafe { Self::new_unchecked(&[]) }
    }

    /// Creates a [`Component`] from a byte slice, without verifying its length.
    ///
    /// This is a cost-free conversion.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than `MCL` may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`Component`].
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let unchecked_component = unsafe { Component::<3>::new_unchecked(b"yay") };
    /// assert_eq!(unchecked_component.as_ref(), b"yay");
    /// ```
    pub unsafe fn new_unchecked<S: AsRef<[u8]> + ?Sized>(s: &S) -> &Self {
        debug_assert!(s.as_ref().len() <= MCL);
        unsafe { &*(s.as_ref() as *const [u8] as *const Self) }
    }
}

impl<const MCL: usize> Deref for Component<MCL> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MCL: usize> AsRef<[u8]> for Component<MCL> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// impl<const MCL: usize> AsRef<Component<MCL>> for Component<MCL> {
//     fn as_ref(&self) -> &Component<MCL> {
//         &self
//     }
// }

impl<const MCL: usize> Borrow<[u8]> for Component<MCL> {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl<const MCL: usize> fmt::Display for Component<MCL> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        FmtHelper(&self).fmt(f)
    }
}

impl<const MCL: usize> fmt::Debug for Component<MCL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Component").field(&FmtHelper(&self)).finish()
    }
}

/// An owned [component](https://willowprotocol.org/specs/data-model/index.html#Component) of a Willow [Path](https://willowprotocol.org/specs/data-model/index.html#Path), using reference counting for cheap cloning. Typically obtained from a [`Path`](super::Path) instead of being created independently.
///
/// This type enforces a const-generic [maximum component length](https://willowprotocol.org/specs/data-model/index.html#max_component_length). Use the [`AsRef`], [`Deref`], or [`Borrow`] implementation to access the immutable byte slice.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OwnedComponent<const MCL: usize>(pub(crate) Bytes);

impl<const MCL: usize> OwnedComponent<MCL> {
    /// Creates an [`OwnedComponent`] by copying data from a byte slice. Returns [`None`] if the slice is longer than `MCL`.
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// assert!(OwnedComponent::<3>::new(b"yay").is_ok());
    /// assert!(OwnedComponent::<3>::new(b"too_long").is_err());
    /// ```
    pub fn new(data: &[u8]) -> Result<Self, InvalidComponentError> {
        if data.len() <= MCL {
            Ok(unsafe { Self::new_unchecked(data) }) // Safe because we just checked the length.
        } else {
            Err(InvalidComponentError)
        }
    }

    /// Creates an [`OwnedComponent`] by copying data from a byte slice, without verifying its length.
    ///
    /// #### Safety
    ///
    /// Supplying a slice of length strictly greater than `MCL` may trigger undefined behavior,
    /// either immediately, or on any subsequent function invocation that operates on the resulting [`OwnedComponent`].
    ///
    /// #### Complexity
    ///
    /// Runs in `O(n)`, where `n` is the length of the slice. Performs a single allocation of `O(n)` bytes.
    ///
    /// #### Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// let unchecked_component = unsafe { OwnedComponent::<3>::new_unchecked(b"yay") };
    /// assert_eq!(unchecked_component.as_ref(), b"yay");
    /// ```
    pub unsafe fn new_unchecked(data: &[u8]) -> Self {
        debug_assert!(data.len() <= MCL);
        Self(Bytes::copy_from_slice(data))
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
    /// use willow_data_model_generic::prelude::*;
    /// let empty_component = OwnedComponent::<3>::new_empty();
    /// assert_eq!(empty_component.as_ref(), &[]);
    /// assert_eq!(empty_component, OwnedComponent::<3>::default());
    /// ```
    pub fn new_empty() -> Self {
        Self(Bytes::new())
    }
}

impl<const MCL: usize> Deref for OwnedComponent<MCL> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<const MCL: usize> AsRef<[u8]> for OwnedComponent<MCL> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<const MCL: usize> Borrow<[u8]> for OwnedComponent<MCL> {
    fn borrow(&self) -> &[u8] {
        self.0.borrow()
    }
}

impl<const MCL: usize> fmt::Display for OwnedComponent<MCL> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        FmtHelper(self).fmt(f)
    }
}

impl<const MCL: usize> fmt::Debug for OwnedComponent<MCL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OwnedComponent")
            .field(&FmtHelper(self))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// An error arising from trying to construct a [`Component`] of length strictly greater than the `MCL` ([max\_component\_length](https://willowprotocol.org/specs/data-model/index.html#max_component_length)).
///
/// #### Example
///
/// ```
/// use willow_data_model_generic::prelude::*;
/// assert_eq!(Component::<4>::new(b"too_long"), Err(InvalidComponentError));
/// ```
pub struct InvalidComponentError;

impl core::fmt::Display for InvalidComponentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Length of a component exceeded the maximum component length"
        )
    }
}

impl std::error::Error for InvalidComponentError {}

struct FmtHelper<'a, T>(&'a T);

impl<'a, T> fmt::Debug for FmtHelper<'a, T>
where
    T: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.as_ref().is_empty() {
            write!(f, "<empty>")
        } else {
            for byte in self.0.as_ref().iter() {
                percent_encode_fmt(f, *byte)?;
            }
            Ok(())
        }
    }
}

fn byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || byte == ('-' as u8)
        || byte == ('.' as u8)
        || byte == ('_' as u8)
        || byte == ('~' as u8)
}

fn percent_encode_fmt(f: &mut fmt::Formatter<'_>, byte: u8) -> fmt::Result {
    if byte_is_unreserved(byte) {
        f.write_char(unsafe { char::from_u32_unchecked(byte as u32) })
    } else {
        f.write_char('%')?;
        let low = byte & 0b0000_1111;
        let high = byte >> 4;
        f.write_char(char::from_digit(high as u32, 16).unwrap())?;
        f.write_char(char::from_digit(low as u32, 16).unwrap())
    }
}

#[test]
fn test_fmt() {
    assert_eq!(&format!("{}", Component::<17>::new("").unwrap()), "<empty>");
    assert_eq!(&format!("{}", Component::<17>::new(b" ").unwrap()), "%20");
    assert_eq!(
        &format!("{}", Component::<17>::new(b".- ~_ab190%/").unwrap()),
        ".-%20~_ab190%25%2f"
    );
}
