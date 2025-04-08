#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::{
    convert::Infallible,
    fmt::{Debug, Display, Formatter},
};

#[cfg(feature = "std")]
use std::error::Error;

use either::Either::*;
use ufotofu::ProduceAtLeastError;

/// The reasons why decoding can fail: the producer might emit its final item too early, it might emit an error, or the received bytes might be problematic.
///
/// `F` is the type of the final item of the producer, `E` is the error type of the producer, and `Other` can describe in arbitrary detail why the decoded bytes were invalid.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError<F, E, Other> {
    UnexpectedEndOfInput(F),
    Producer(E),
    Other(Other),
}

impl<F, E, OtherB> DecodeError<F, E, OtherB> {
    /// Maps from one `DecodeError` to another `DecodeError` by leaving producer errors and unexpected ends of input untouched but mapping other errors via a `From` implementation.
    pub fn map_other_from<OtherA>(err: DecodeError<F, E, OtherA>) -> Self
    where
        OtherB: From<OtherA>,
    {
        match err {
            DecodeError::Producer(err) => DecodeError::Producer(err),
            DecodeError::UnexpectedEndOfInput(fin) => DecodeError::UnexpectedEndOfInput(fin),
            DecodeError::Other(noncanonic_err) => DecodeError::Other(noncanonic_err.into()),
        }
    }

    /// Maps from one `DecodeError` to another `DecodeError` by leaving producer errors and unexpected ends of input untouched but mapping other errors via the given function.
    pub fn map_other<OtherA, Fun>(err: DecodeError<F, E, OtherA>, fun: Fun) -> Self
    where
        Fun: FnOnce(OtherA) -> OtherB,
    {
        match err {
            DecodeError::Producer(err) => DecodeError::Producer(err),
            DecodeError::UnexpectedEndOfInput(fin) => DecodeError::UnexpectedEndOfInput(fin),
            DecodeError::Other(noncanonic_err) => DecodeError::Other(fun(noncanonic_err)),
        }
    }
}

impl<F, E, Other> From<E> for DecodeError<F, E, Other> {
    fn from(err: E) -> Self {
        DecodeError::Producer(err)
    }
}

impl<F, E, Other> From<ProduceAtLeastError<F, E>> for DecodeError<F, E, Other> {
    fn from(err: ProduceAtLeastError<F, E>) -> Self {
        match err.reason {
            Left(fin) => DecodeError::UnexpectedEndOfInput(fin),
            Right(err) => DecodeError::Producer(err),
        }
    }
}

impl<F: Display, E: Display, Other: Display> Display for DecodeError<F, E, Other> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::Producer(err) => {
                write!(f, "The underlying producer encountered an error: {}", err,)
            }
            DecodeError::UnexpectedEndOfInput(fin) => {
                write!(
                    f,
                    "The underlying producer emitted its final value: {}",
                    fin
                )
            }
            DecodeError::Other(reason) => {
                write!(f, "Failed to decode: {}", reason)
            }
        }
    }
}

#[cfg(feature = "std")]
impl<F, E, Other> Error for DecodeError<F, E, Other>
where
    F: Display + Debug,
    E: 'static + Error,
    Other: 'static + Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::Producer(err) => Some(err),
            DecodeError::UnexpectedEndOfInput(_fin) => None,
            DecodeError::Other(reason) => Some(reason),
        }
    }
}

/// An error reason for minimalistic decoding error handling: only tracks whether decoding failed because of invalid input or because of limitations of the decoding implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Blame {
    /// Received an incorrect encoding.
    TheirFault,
    /// Received a valid encoding which we couldn't handle. Typical reasons are values that do not fit into a `usize` or running out of memory.
    OurFault,
}

impl Blame {
    /// Converts from a `u64` to a `usize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `usize`.
    pub fn u64_to_usize<F, E>(n: u64) -> Result<usize, DecodeError<F, E, Blame>> {
        usize::try_from(n).map_err(|_| DecodeError::Other(Blame::OurFault))
    }

    /// Converts from a `u32` to a `usize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `usize`.
    pub fn u32_to_usize<F, E>(n: u32) -> Result<usize, DecodeError<F, E, Blame>> {
        usize::try_from(n).map_err(|_| DecodeError::Other(Blame::OurFault))
    }

    /// Converts from a `u16` to a `usize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `usize`.
    pub fn u16_to_usize<F, E>(n: u16) -> Result<usize, DecodeError<F, E, Blame>> {
        Ok(usize::from(n))
    }

    /// Converts from an `i64` to a `isize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `isize`.
    pub fn i64_to_usize<F, E>(n: i64) -> Result<isize, DecodeError<F, E, Blame>> {
        isize::try_from(n).map_err(|_| DecodeError::Other(Blame::OurFault))
    }

    /// Converts from an `i32` to a `isize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `isize`.
    pub fn i32_to_usize<F, E>(n: i32) -> Result<isize, DecodeError<F, E, Blame>> {
        isize::try_from(n).map_err(|_| DecodeError::Other(Blame::OurFault))
    }

    /// Converts from an `i16` to a `isize`, yielding a `DecodeError::Other(Blame::OurFault)` if the number does not fit into a `isize`.
    pub fn i16_to_usize<F, E>(n: i16) -> Result<isize, DecodeError<F, E, Blame>> {
        Ok(isize::from(n))
    }
}

impl From<Infallible> for Blame {
    fn from(_value: Infallible) -> Self {
        unreachable!("It's impossible to call this function!")
    }
}

impl Display for Blame {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Blame::TheirFault => {
                write!(f, "Received an incorrect encoding.")
            }
            Blame::OurFault => {
                write!(f, "Received an encoding we could not process.")
            }
        }
    }
}

#[cfg(feature = "std")]
impl Error for Blame {}
