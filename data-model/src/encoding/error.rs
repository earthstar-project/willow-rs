use either::Either;
use std::num::TryFromIntError;
use ufotofu::{
    common::errors::{ConsumeFullSliceError, OverwriteFullSliceError},
    local_nb::producer::PipeIntoSliceError,
};

/// Everything that can go wrong when decoding a value.
#[derive(Debug)]
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

impl<E> From<ConsumeFullSliceError<E>> for DecodeError<E> {
    fn from(value: ConsumeFullSliceError<E>) -> Self {
        DecodeError::Producer(value.reason)
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
