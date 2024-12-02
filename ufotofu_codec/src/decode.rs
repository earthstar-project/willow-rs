#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::convert::Infallible;
use core::future::Future;

use ufotofu::{producer::FromSlice, BulkProducer};

use crate::DecodeError;

/// Methods for decoding a value that belongs to an *encoding relation*.
///
/// API contracts:
///
/// - The result of decoding must depend only on the decoded bytes, not on details of the producer such as when it yields or how many items it exposes at a time.
/// - For types that also implement `Encodable` and `Eq`, encoding a value and then decoding it must yield a value equal to the original.
/// - `decode` must not read any bytes beyond the end of the encoding.
pub trait Decodable: Sized {
    /// Reason why decoding can fail (beyond an unexpected end of input or a producer error).
    type ErrorReason;

    /// Decodes the bytes produced by the given producer into a `Self`, or yields an error if the producer does not produce a valid encoding.
    fn decode<P>(
        producer: &mut P,
    ) -> impl Future<Output = Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer.
    fn decode_from_slice(
        enc: &[u8],
    ) -> impl Future<Output = Result<Self, DecodeError<(), Infallible, Self::ErrorReason>>> {
        async { Self::decode(&mut FromSlice::new(enc)).await }
    }
}

/// Decoding for an *encoding relation* with a one-to-one mapping between values and their codes (i.e., the relation is a [bijection](https://en.wikipedia.org/wiki/Bijection)).
///
/// This may specialise an arbitrary encoding relation to implement a canonic subset.
///
/// API contracts:
///
/// - Two nonequal codes must not decode to the same value with `decode_canonic`.
/// - Any code that decodes via `decode_canonic` must also decode via `decode`.
///
/// There is no corresponding `EncodableCanonic` trait, because `Encodable` already fulfils the dual requirement of two nonequal values yielding nonequal codes.
pub trait DecodableCanonic: Decodable {
    /// The type for reporting that the bytestring to decode was not a valid canonic encoding of any value of type `Self`.
    ///
    /// Typically contains at least as much information as [`Self::Error`](Decodable::Error). If the encoding relation implemented by [`Decodable`] is already canonic, then [`ErrorCanonic`](DecodableCanonic::ErrorCanonic) should be equal to [`Self::Error`](Decodable::Error).
    type ErrorCanonic: From<Self::ErrorReason>;

    /// Decodes the bytes produced by the given producer into a `Self`, and errors if the input encoding is not the canonical one.
    fn decode_canonic<P>(
        producer: &mut P,
    ) -> impl Future<Output = Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn decode_canonic_from_slice(
        enc: &[u8],
    ) -> impl Future<Output = Result<Self, DecodeError<(), Infallible, Self::ErrorCanonic>>> {
        async { Self::decode_canonic(&mut FromSlice::new(enc)).await }
    }
}

/// A decodable that introduces no asynchrony beyond that of `.await`ing the producer. This is essentially a marker trait by which to tell other programmers about this property. As a practical benefit, the default methods of this trait allow for convenient synchronous decoding via producers that are known to never block.
pub trait DecodableSync: Decodable {
    /// Synchronously decodes from a slice instead of a producer.
    fn sync_decode_from_slice(
        enc: &[u8],
    ) -> Result<Self, DecodeError<(), Infallible, Self::ErrorReason>> {
        pollster::block_on(Self::decode_from_slice(enc))
    }

    /// Synchronously and absolutely decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn sync_decode_canonic_from_slice(
        enc: &[u8],
    ) -> Result<Self, DecodeError<(), Infallible, Self::ErrorCanonic>>
    where
        Self: DecodableCanonic,
    {
        pollster::block_on(Self::decode_canonic_from_slice(enc))
    }
}
