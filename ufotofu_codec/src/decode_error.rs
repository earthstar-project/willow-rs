#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::fmt::{Debug, Display, Formatter};

#[cfg(feature = "std")]
use std::error::Error;

use either::Either::*;
use ufotofu::OverwriteFullSliceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError<F, E, Other> {
    UnexpectedEndOfInput(F),
    Producer(E),
    Other(Other),
}

impl<F, E, Other> From<E> for DecodeError<F, E, Other> {
    fn from(err: E) -> Self {
        DecodeError::Producer(err)
    }
}

impl<F, E, Other> From<OverwriteFullSliceError<F, E>> for DecodeError<F, E, Other> {
    fn from(err: OverwriteFullSliceError<F, E>) -> Self {
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
