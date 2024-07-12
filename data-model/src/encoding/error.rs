use either::Either;
use std::num::TryFromIntError;
use ufotofu::{
    common::errors::{ConsumeFullSliceError, OverwriteFullSliceError},
    local_nb::producer::PipeIntoSliceError,
};

/// Returned when a encoding fails to be consumed by a [`ufotofu::local_nb::Consumer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingConsumerError<E> {
    /// The number of bytes which were consumed before the error.
    pub bytes_consumed: usize,
    /// The error returned on the final and failed attempt to consume bytes.
    pub reason: E,
}

impl<E> From<ConsumeFullSliceError<E>> for EncodingConsumerError<E> {
    fn from(err: ConsumeFullSliceError<E>) -> Self {
        EncodingConsumerError {
            bytes_consumed: err.consumed,
            reason: err.reason,
        }
    }
}

/// Everything that can go wrong when decoding a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError<ProducerError> {
    /// The producer of the bytes to be decoded errored somehow.
    Producer(ProducerError),
    /// The bytes produced by the producer cannot be decoded into anything meaningful.
    InvalidInput,
    /// Tried to use a u64 as a usize when the current target's usize is not big enough.
    U64DoesNotFitUsize,
}

impl<'a, T, F, E> From<PipeIntoSliceError<'a, T, F, E>> for DecodeError<E> {
    fn from(value: PipeIntoSliceError<'a, T, F, E>) -> Self {
        match value.reason {
            Either::Left(_) => DecodeError::InvalidInput,
            Either::Right(err) => DecodeError::Producer(err),
        }
    }
}

impl<'a, T, F, E> From<OverwriteFullSliceError<'a, T, F, E>> for DecodeError<E> {
    fn from(value: OverwriteFullSliceError<'a, T, F, E>) -> Self {
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
