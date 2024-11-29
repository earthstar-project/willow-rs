#![no_std]

//! # UFOTOFU Codec
//!
//! Traits for (streaming) encoding and decoding of values to and from bytestrings using [UFOTOFU](https://crates.io/crates/ufotofu).
//!
//! Intuitively, we request the following properties of encodings:
//!
//! - each value has at least one encoding (but possibly more),
//! - no two values have the same encoding, and
//! - no encoding is a prefix of another encoding.
//!
//! Formally, we say an *encoding relation* is a set of pairs (i.e., a [binary relation](https://en.wikipedia.org/wiki/Binary_relation)) `(t, enc)` of a value of type `T` and a finite bytestring, such that
//!
//! - for each `t` in `T` there is at least one pair in the relation that contains `t` (i.e., the relation is a [total function](https://en.wikipedia.org/wiki/Function_(mathematics))),
//! - there are no two distinct `t1, t2` in `T` such that there is a single encoding `enc` such that `(t1, enc)` and `(t2, enc)` are both in the relation (i.e., the relation is [injective](https://en.wikipedia.org/wiki/Injective_function)), and
//! - for any two bytestrings `enc1, enc2` in the relation, neither is a strict prefix of the other (i.e., we have a [prefix code](https://en.wikipedia.org/wiki/Prefix_code)).
//!
//! The [`Encodable`] and [`Decodable`] trait describe how to convert between the domain of encodable values and the domain of finite bytestrings. Further traits specialise these:
//!
//! - [`EncodableCanonic`] and [`DecodableCanonic`] allow working with canonic subsets of an encoding relation, i.e., encodings where there is a one-to-one correspondence between values and codes.
//! - [`EncodableSync`] and [`DecodableSync`] are marker traits that signal that all asynchrony in encoding/decoding stems from the consumer/producer, but all further computations are synchronous. When using a consumer/producer that never blocks either, this allows for synchronous encoding/decoding.
//! - [`EncodableKnownSize`] describes how to compute the size of an encoding without actually performing the encoding. Useful when the size needs to be known in advance (length-prefixing, or encoding into a slice).

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::fmt::Display;
use core::fmt::Formatter;
use core::future::Future;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
#[cfg(all(feature = "std", not(feature = "alloc")))]
use std::{boxed::Box, collections::Vec};

#[cfg(feature = "std")]
use std::error::Error;

use either::Either::*;
use ufotofu::{BulkConsumer, BulkProducer, OverwriteFullSliceError};

/// A generic error type to use as a [`Decodable::Error`] when more detailed error reporting is not necessary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError<ProducerError> {
    /// The producer of the bytes to be decoded errored somehow.
    Producer(ProducerError),
    /// The bytes produced by the producer are not a valid encoding. A common subcase is when the producer emits its final item too soon.
    InvalidInput,
    /// The producer produced a (valid prefix) of an encoding, but we cannot represent the decoded value. A typical example would be decoding a number that does not fit into a `usize`, another typical case is running out of memory.
    Irrepresentable,
}

/// Turn an `OverwriteFullSliceError` into an appropriate `DecodeError`: [`DecodeError::InvalidInput`] if the final item was emitted too early, and [`DecodeError::Producer`] if the producer emitted an error.
impl<F, E> From<OverwriteFullSliceError<F, E>> for DecodeError<E> {
    /// Turn an `OverwriteFullSliceError` into an appropriate `DecodeError`: [`DecodeError::InvalidInput`] if the final item was emitted too early, and [`DecodeError::Producer`] if the producer emitted an error.
    fn from(value: OverwriteFullSliceError<F, E>) -> Self {
        match value.reason {
            Left(_) => DecodeError::InvalidInput,
            Right(err) => DecodeError::Producer(err),
        }
    }
}

impl<ProducerError> From<ProducerError> for DecodeError<ProducerError> {
    fn from(err: ProducerError) -> Self {
        DecodeError::Producer(err)
    }
}

#[cfg(feature = "std")]
impl<E> Error for DecodeError<E>
where
    E: 'static + Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::Producer(err) => Some(err),
            DecodeError::InvalidInput => None,
            DecodeError::Irrepresentable => None,
        }
    }
}

impl<E: Display> Display for DecodeError<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::Producer(err) => {
                write!(f, "The underlying producer encountered an error: {}", err,)
            }
            DecodeError::InvalidInput => {
                write!(
                    f,
                    "Decoding failed due to receiving invalid input, i.e., not a valid encoding.",
                )
            }
            DecodeError::Irrepresentable => {
                write!(f, "Decoded a prefix of a valid encoding such that the resulting value cannot be represented.",)
            }
        }
    }
}

/// Methods for decoding a value that belongs to an *encoding relation* (see the module-level docs for the exact definition).
pub trait Decodable: Sized {
    type Error;

    /// Decodes the bytes produced by the given producer into a `Self`, or yields an error if the producer does not produce a valid encoding.
    fn decode<P>(producer: &mut P) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer.
    fn decode_from_slice(enc: &[u8]) -> impl Future<Output = Result<Self, Self::Error>> {
        async { unimplemented!() }
    }
}

/// Methods for encoding a value that belongs to an *encoding relation* (see the module-level docs for the exact definition).
pub trait Encodable {
    /// Writes an encoding of `&self` into the given consumer.
    fn encode<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>;

    #[cfg(feature = "alloc")]
    /// Encodes into a Vec instead of a given consumer.
    fn encode_into_vec(&self) -> impl Future<Output = Vec<u8>> {
        async { unimplemented!() }
    }
}

/// Encodables that can (efficiently and synchronously) precompute the length of their encoding.
pub trait EncodableKnownSize: Encodable {
    /// Computes the size of the encoding in bytes. Calling [`encode`](Encodable::encode) must feed exactly that many bytes into the consumer.
    fn len_of_encoding(&self) -> usize;

    #[cfg(feature = "alloc")]
    /// Encodes into a boxed slice instead of a given consumer.
    fn encode_into_boxed_slice(&self) -> impl Future<Output = Box<[u8]>> {
        async { unimplemented!() }
    }
}

/// Decoding for an *encoding relation* with a one-to-one mapping between values and their codes (i.e., the relation is a [bijection](https://en.wikipedia.org/wiki/Bijection)).
///
/// This may specialise an arbitrary encoding relation to implement a canonic subset.
pub trait DecodableCanonic: Decodable {
    type ErrorCanonic: From<Self::Error>;

    /// Decodes the bytes produced by the given producer into a `Self`, and errors if the input encoding is not the canonical one.
    fn decode_canonic<P>(producer: &mut P) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn decode_canonic_from_slice(enc: &[u8]) -> impl Future<Output = Result<Self, Self::Error>> {
        async { unimplemented!() }
    }
}

/// Encoding for an *encoding relation* with a one-to-one mapping between values and their codes (i.e., the relation is a [bijection](https://en.wikipedia.org/wiki/Bijection)).
///
/// This may specialise an arbitrary encoding relation to implement a canonic subset.
///
/// The default implementation simply calls [`self.encode`](Encodable::encode), since usually that function is deterministic and produces the canonic encoding already.
pub trait EncodableCanonic: Encodable {
    /// Writes the canonic encoding of `&self` into the given consumer.
    fn encode_canonic<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.encode(consumer)
    }

    #[cfg(feature = "alloc")]
    /// Encodes the canonic encoding of `&self` into a Vec instead of a given consumer.
    fn encode_canonic_into_vec(&self) -> impl Future<Output = Vec<u8>> {
        async { unimplemented!() }
    }

    #[cfg(feature = "alloc")]
    /// Encodes the canonic encoding of `&self` into a boxed slice instead of a given consumer.
    fn encode_canonic_into_boxed_slice(&self) -> impl Future<Output = Box<[u8]>>
    where
        Self: EncodableKnownSize,
    {
        async { unimplemented!() }
    }
}

/// A decodable that introduces no asynchrony beyond that of `.await`ing the producer.
///
/// If the producer is known to not block either, this enables synchronous decoding.
pub trait DecodableSync: Decodable {
    /// Synchronously decodes from a slice instead of a producer.
    fn sync_decode_from_slice(enc: &[u8]) -> Result<Self, Self::Error> {
        unimplemented!()
    }

    /// Asynchronously decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn sync_decode_canonic_from_slice(enc: &[u8]) -> Result<Self, Self::Error>
    where
        Self: DecodableCanonic,
    {
        unimplemented!()
    }
}

/// An encodable that introduces no asynchrony beyond that of `.await`ing the consumer.
///
/// If the consumer is known to not block either, this enables synchronous encoding.
pub trait EncodableSync: Encodable {
    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a Vec instead of a given consumer.
    fn sync_encode_into_vec(&self) -> Vec<u8> {
        unimplemented!()
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a boxed slice instead of a given consumer.
    fn sync_encode_into_boxed_slice(&self) -> Box<[u8]>
    where
        Self: EncodableKnownSize,
    {
        unimplemented!()
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes the canonic encoding of `&self` into a Vec instead of a given consumer.
    fn sync_encode_canonic_into_vec(&self) -> Vec<u8>
    where
        Self: EncodableCanonic,
    {
        unimplemented!()
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes the canonic encoding of `&self` into a boxed slice instead of a given consumer.
    fn sync_encode_canonic_into_boxed_slice(&self) -> Box<[u8]>
    where
        Self: EncodableKnownSize + EncodableCanonic,
    {
        unimplemented!()
    }
}

// TODO Relativity
