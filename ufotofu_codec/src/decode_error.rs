#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::fmt::{Debug, Display, Formatter};

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
    /// Map from one `DecodeError` to another `DecodeError` by leaving producer errors and unexpected ends of input untouched but mapping other errors via a `From` implementation.
    pub fn map_other<OtherA>(err: DecodeError<F, E, OtherA>) -> Self
    where
        OtherB: From<OtherA>,
    {
        match err {
            DecodeError::Producer(err) => DecodeError::Producer(err),
            DecodeError::UnexpectedEndOfInput(fin) => DecodeError::UnexpectedEndOfInput(fin),
            DecodeError::Other(noncanonic_err) => DecodeError::Other(noncanonic_err.into()),
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
