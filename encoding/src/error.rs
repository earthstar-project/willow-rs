use core::{fmt::Display, fmt::Formatter, num::TryFromIntError};
use either::Either;
use std::error::Error;
use ufotofu::common::errors::OverwriteFullSliceError;

/// Everything that can go wrong when decoding a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError<ProducerError> {
    /// The producer of the bytes to be decoded errored somehow.
    Producer(ProducerError),
    /// The bytes produced by the producer cannot be decoded into anything meaningful.
    InvalidInput,
    /// Tried to use a u64 as a usize when the current compilation target's usize is not big enough.
    U64DoesNotFitUsize,
}

impl<F, E> From<OverwriteFullSliceError<F, E>> for DecodeError<E> {
    fn from(value: OverwriteFullSliceError<F, E>) -> Self {
        match value.reason {
            Either::Left(_) => DecodeError::InvalidInput,
            Either::Right(err) => DecodeError::Producer(err),
        }
    }
}

impl<ProducerError> From<TryFromIntError> for DecodeError<ProducerError> {
    fn from(_: TryFromIntError) -> Self {
        DecodeError::U64DoesNotFitUsize
    }
}

impl<E> Error for DecodeError<E>
where
    E: 'static + Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::Producer(err) => Some(err),
            DecodeError::InvalidInput => None,
            DecodeError::U64DoesNotFitUsize => None,
        }
    }
}

impl<E> Display for DecodeError<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::Producer(_) => {
                write!(f, "The underlying producer encountered an error",)
            }
            DecodeError::InvalidInput => {
                write!(f, "Decoding failed due to receiving invalid input",)
            }
            DecodeError::U64DoesNotFitUsize => {
                write!(f, "Tried (and failed) to decode a u64 to a 32-bit usize",)
            }
        }
    }
}
