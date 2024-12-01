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
//! ## Traits
//!
//! The [`Encodable`] and [`Decodable`] trait describe how to convert between the domain of encodable values and the domain of finite bytestrings. Further traits specialise these:
//!
//! - [`EncodableCanonic`] and [`DecodableCanonic`] allow working with canonic subsets of an encoding relation, i.e., encodings where there is a one-to-one correspondence between values and codes.
//! - [`EncodableSync`] and [`DecodableSync`] are marker traits that signal that all asynchrony in encoding/decoding stems from the consumer/producer, but all further computations are synchronous. When using a consumer/producer that never blocks either, this allows for synchronous encoding/decoding.
//! - [`EncodableKnownSize`] describes how to compute the size of an encoding without actually performing the encoding. Useful when the size needs to be known in advance (length-prefixing, or encoding into a slice).
//!
//! ## Relativity
//!
//! All traits support encoding and decoding *relative* to some reference value that is supplied dynamically at runtime. For absolute encodings, set the type of the reference value to `()`; this enables convenience methods that do not require passing a `&()` all the time.
//!
//! Formally, each possible reference value defines an encoding relation that is independent from the relations defined by any other reference values. All API contracts that are stated in the documentation must hold for the same reference value. As an example, consider an encoding for integers that encodes relative to a reference value by encoding the difference between the value to encode and the reference value. Then, when encoding any two distinct values relative to the reference value `5`, the resulting encodings must be different. But when encoding some value `x` relative to `5` and another value `y` relative to `17`, then it is perfectly fine for both encodings to be the same bytestring.
//!
//! ## Encoders and Decoders
//!
//! TODO
//!
//! ## Property Testing
//!
//! When the `dev` feature is enabled, the [`proptest`] module provides helpers for writing property tests for checking the invariants that implementations of the traits of this crate must uphold.

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "dev")]
pub mod proptest;

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
use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, OverwriteFullSliceError,
};

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

/// Methods for decoding a value that belongs to an *encoding relation*.
///
/// API contracts:
///
/// - The result of decoding must depend only on the decoded bytes, not on details of the producer such as when it yields or how many items it exposes at a time.
/// - For types that also implement `Encodable` and `Eq`, encoding a value and then decoding it must yield a value equal to the original.
pub trait Decodable: Sized {
    /// The type for reporting that the bytestring to decode was not a valid encoding of any value of type `Self`.
    ///
    /// The [`DecodeError`] type is a recommended choice for this type parameter when it is not important to trace in detail why exactly decoding went wrong.
    type Error;

    /// The type of values relative to which we decode. Set to `()` for absolute encodings.
    type DecodeRelativeTo;

    /// Decodes the bytes produced by the given producer into a `Self`, or yields an error if the producer does not produce a valid encoding.
    fn relative_decode<P>(
        producer: &mut P,
        r: &Self::DecodeRelativeTo,
    ) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes the bytes produced by the given producer into a `Self`, or yields an error if the producer does not produce a valid encoding.
    fn decode<P>(producer: &mut P) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        Self::relative_decode(producer, &())
    }

    /// Decodes from a slice instead of a producer.
    fn relative_decode_from_slice(
        enc: &[u8],
        r: &Self::DecodeRelativeTo,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        async { Self::relative_decode(&mut FromSlice::new(enc), r).await }
    }

    /// Decodes from a slice instead of a producer.
    fn decode_from_slice(enc: &[u8]) -> impl Future<Output = Result<Self, Self::Error>>
    where
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        Self::relative_decode_from_slice(enc, &())
    }
}

/// Methods for encoding a value that belongs to an *encoding relation*.
///
/// API contracts:
///
/// - The encoding must not depend on details of the consumer such as when it yields or how many item slots it exposes at a time.
/// - Nonequal values must result in nonequal encodings.
/// - No encoding must be a prefix of a different encoding.
/// - For types that also implement `Decodable` and `Eq`, encoding a value and then decoding it must yield a value equal to the original.
pub trait Encodable {
    /// The type of values relative to which we encode. Set to `()` for absolute encodings.
    type EncodeRelativeTo;

    /// Writes an encoding of `&self` into the given consumer.
    fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Self::EncodeRelativeTo,
    ) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>;

    /// Writes an absolute encoding of `&self` into the given consumer.
    fn encode<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>,
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        self.relative_encode(consumer, &())
    }

    #[cfg(feature = "alloc")]
    /// Encodes into a Vec instead of a given consumer.
    fn relative_encode_into_vec(
        &self,
        r: &Self::EncodeRelativeTo,
    ) -> impl Future<Output = Vec<u8>> {
        async {
            let mut c = IntoVec::new();

            match self.relative_encode(&mut c, r).await {
                Ok(()) => c.into_vec(),
                Err(_) => unreachable!(),
            }
        }
    }

    #[cfg(feature = "alloc")]
    /// Absolutely encodes into a Vec instead of a given consumer.
    fn encode_into_vec(&self) -> impl Future<Output = Vec<u8>>
    where
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        self.relative_encode_into_vec(&())
    }
}

/// Encodables that can (efficiently and synchronously) precompute the length of their encoding.
///
/// API contract: `self.encode(c)` must write exactly `self.len_of_encoding()` many bytes into `c`.
pub trait EncodableKnownSize: Encodable {
    /// Computes the size of the encoding in bytes. Calling [`relative_encode`](Encodable::relative_encode) must feed exactly that many bytes into the consumer.
    fn relative_len_of_encoding(&self, r: &Self::EncodeRelativeTo) -> usize;

    /// Computes the size of the absolute encoding in bytes. Calling [`encode`](Encodable::encode) must feed exactly that many bytes into the consumer.
    fn len_of_encoding(&self) -> usize
    where
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        self.relative_len_of_encoding(&())
    }

    #[cfg(feature = "alloc")]
    /// Encodes into a boxed slice instead of a given consumer.
    fn relative_encode_into_boxed_slice(
        &self,
        r: &Self::EncodeRelativeTo,
    ) -> impl Future<Output = Box<[u8]>> {
        async {
            let mut c = IntoVec::with_capacity(self.relative_len_of_encoding(r));

            match self.relative_encode(&mut c, r).await {
                Ok(()) => c.into_vec().into_boxed_slice(),
                Err(_) => unreachable!(),
            }
        }
    }

    #[cfg(feature = "alloc")]
    /// Absolutely encodes into a boxed slice instead of a given consumer.
    fn encode_into_boxed_slice(&self) -> impl Future<Output = Box<[u8]>>
    where
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        self.relative_encode_into_boxed_slice(&())
    }
}

/// Decoding for an *encoding relation* with a one-to-one mapping between values and their codes (i.e., the relation is a [bijection](https://en.wikipedia.org/wiki/Bijection)).
///
/// This may specialise an arbitrary encoding relation to implement a canonic subset.
///
/// API contract: Two nonequal codes must not decode to the same value with `decode_canonic`.
///
/// There is no corresponding `EncodableCanonic` trait, because `Encodable` already fulfils the dual requirement of two nonequal values yielding nonequal codes.
pub trait DecodableCanonic: Decodable {
    /// The type for reporting that the bytestring to decode was not a valid canonic encoding of any value of type `Self`.
    ///
    /// Typically contains at least as much information as [`Self::Error`](Decodable::Error). If the encoding relation implemented by [`Decodable`] is already canonic, then [`ErrorCanonic`](DecodableCanonic::ErrorCanonic) should be equal to [`Self::Error`](Decodable::Error).
    type ErrorCanonic: From<Self::Error>;

    /// Decodes the bytes produced by the given producer into a `Self`, and errors if the input encoding is not the canonical one.
    fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Self::DecodeRelativeTo,
    ) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    ///Absolutely decodes the bytes produced by the given producer into a `Self`, and errors if the input encoding is not the canonical one.
    fn decode_canonic<P>(producer: &mut P) -> impl Future<Output = Result<Self, Self::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        Self::relative_decode_canonic(producer, &())
    }

    /// Decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn relative_decode_canonic_from_slice(
        enc: &[u8],
        r: &Self::DecodeRelativeTo,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        async { Self::relative_decode_canonic(&mut FromSlice::new(enc), r).await }
    }

    /// Absolutely decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn decode_canonic_from_slice(enc: &[u8]) -> impl Future<Output = Result<Self, Self::Error>>
    where
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        Self::relative_decode_canonic_from_slice(enc, &())
    }
}

/// A decodable that introduces no asynchrony beyond that of `.await`ing the producer. This is essentially a marker trait by which to tell other programmers about this property. As a practical benefit, the default methods of this trait allow for convenient synchronous decoding via producers that are known to never block.
pub trait DecodableSync: Decodable {
    /// Synchronously decodes from a slice instead of a producer.
    fn sync_relative_decode_from_slice(
        enc: &[u8],
        r: &Self::DecodeRelativeTo,
    ) -> Result<Self, Self::Error> {
        pollster::block_on(Self::relative_decode_from_slice(enc, r))
    }

    /// Synchronously and absolutely decodes from a slice instead of a producer.
    fn sync_decode_from_slice(enc: &[u8]) -> Result<Self, Self::Error>
    where
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        pollster::block_on(Self::decode_from_slice(enc))
    }

    /// Synchronously decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn sync_relative_decode_canonic_from_slice(
        enc: &[u8],
        r: &Self::DecodeRelativeTo,
    ) -> Result<Self, Self::Error>
    where
        Self: DecodableCanonic,
    {
        pollster::block_on(Self::relative_decode_canonic_from_slice(enc, r))
    }

    /// Synchronously and absolutely decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn sync_decode_canonic_from_slice(enc: &[u8]) -> Result<Self, Self::Error>
    where
        Self: DecodableCanonic,
        Self: Decodable<DecodeRelativeTo = ()>,
    {
        pollster::block_on(Self::decode_canonic_from_slice(enc))
    }
}

/// An encodable that introduces no asynchrony beyond that of `.await`ing the consumer.
///
/// If the consumer is known to not block either, this enables synchronous encoding.
pub trait EncodableSync: Encodable {
    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a Vec instead of a given consumer.
    fn sync_relative_encode_into_vec(&self, r: &Self::EncodeRelativeTo) -> Vec<u8> {
        pollster::block_on(self.relative_encode_into_vec(r))
    }

    /// Synchronously and absolutely encodes into a Vec instead of a given consumer.
    fn sync_encode_into_vec(&self) -> Vec<u8>
    where
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        pollster::block_on(self.encode_into_vec())
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a boxed slice instead of a given consumer.
    fn sync_relative_encode_into_boxed_slice(&self, r: &Self::EncodeRelativeTo) -> Box<[u8]>
    where
        Self: EncodableKnownSize,
    {
        pollster::block_on(self.relative_encode_into_boxed_slice(r))
    }

    #[cfg(feature = "alloc")]
    /// Synchronously and absolutely encodes into a boxed slice instead of a given consumer.
    fn sync_encode_into_boxed_slice(&self) -> Box<[u8]>
    where
        Self: EncodableKnownSize,
        Self: Encodable<EncodeRelativeTo = ()>,
    {
        pollster::block_on(self.encode_into_boxed_slice())
    }
}

// TODO Encoder, Decoder, DecoderCanonic; absolute and relative; (owned and borrowed for the relative ones?)
