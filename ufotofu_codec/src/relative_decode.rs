#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::convert::Infallible;
use core::future::Future;

use ufotofu::{producer::FromSlice, BulkProducer};

use crate::*;

/// Methods for decoding relative to some known value of type `RelativeTo`.
///
/// API contracts:
///
/// - The result of decoding must depend only on the decoded bytes, not on details of the producer such as when it yields or how many items it exposes at a time.
/// - For types that also implement `Encodable` and `Eq`, encoding a value and then decoding it must yield a value equal to the original.
pub trait RelativeDecodable<RelativeTo, ErrorReason>: Sized {
    /// Decodes the bytes produced by the given producer into a `Self`, or yields an error if the producer does not produce a valid encoding.
    fn relative_decode<P>(
        producer: &mut P,
        r: &RelativeTo,
    ) -> impl Future<Output = Result<Self, DecodeError<P::Final, P::Error, ErrorReason>>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer.
    fn relative_decode_from_slice(
        enc: &[u8],
        r: &RelativeTo,
    ) -> impl Future<Output = Result<Self, DecodeError<(), Infallible, ErrorReason>>> {
        async { Self::relative_decode(&mut FromSlice::new(enc), r).await }
    }
}

impl<T> RelativeDecodable<(), T::ErrorReason> for T
where
    T: Decodable,
{
    fn relative_decode<P>(
        producer: &mut P,
        _r: &(),
    ) -> impl Future<Output = Result<Self, DecodeError<P::Final, P::Error, <T as Decodable>::ErrorReason>>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer)
    }
}

/// Relative decoding for an *encoding relation* with a one-to-one mapping between values and their codes (i.e., the relation is a [bijection](https://en.wikipedia.org/wiki/Bijection)).
///
/// This may specialise an arbitrary encoding relation to implement a canonic subset. `ErrorCanonic` is the type for reporting that the bytestring to decode was not a valid canonic encoding of any value of type `Self`. It typically contains at least as much information as [`Self::Error`](Decodable::Error). If the encoding relation implemented by [`Decodable`] is already canonic, then [`ErrorCanonic`] should be equal to [`ErrorReason`].
///
/// API contract: Two nonequal codes must not decode to the same value with `decode_canonic`.
///
/// There is no corresponding `EncodableCanonic` trait, because `Encodable` already fulfils the dual requirement of two nonequal values yielding nonequal codes.
pub trait RelativeDecodableCanonic<RelativeTo, ErrorReason, ErrorCanonic>:
    RelativeDecodable<RelativeTo, ErrorReason>
where
    ErrorCanonic: From<ErrorReason>,
{
    /// Decodes the bytes produced by the given producer into a `Self`, and errors if the input encoding is not the canonical one.
    fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &RelativeTo,
    ) -> impl Future<Output = Result<Self, DecodeError<P::Final, P::Error, ErrorCanonic>>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn relative_decode_canonic_from_slice(
        enc: &[u8],
        r: &RelativeTo,
    ) -> impl Future<Output = Result<Self, DecodeError<(), Infallible, ErrorCanonic>>> {
        async { Self::relative_decode_canonic(&mut FromSlice::new(enc), r).await }
    }
}

impl<T> RelativeDecodableCanonic<(), T::ErrorReason, T::ErrorCanonic> for T
where
    T: DecodableCanonic,
{
    fn relative_decode_canonic<P>(
        producer: &mut P,
        _r: &(),
    ) -> impl Future<
        Output = Result<
            Self,
            DecodeError<P::Final, P::Error, <T as DecodableCanonic>::ErrorCanonic>,
        >,
    >
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode_canonic(producer)
    }
}

/// A relative decodable that introduces no asynchrony beyond that of `.await`ing the producer. This is essentially a marker trait by which to tell other programmers about this property. As a practical benefit, the default methods of this trait allow for convenient synchronous decoding via producers that are known to never block.
pub trait RelativeDecodableSync<RelativeTo, ErrorReason>:
    RelativeDecodable<RelativeTo, ErrorReason>
{
    /// Synchronously decodes from a slice instead of a producer.
    fn sync_relative_decode_from_slice(
        enc: &[u8],
        r: &RelativeTo,
    ) -> Result<Self, DecodeError<(), Infallible, ErrorReason>> {
        pollster::block_on(Self::relative_decode_from_slice(enc, r))
    }

    /// Synchronously decodes from a slice instead of a producer, and errors if the input encoding is not the canonical one.
    fn sync_relative_decode_canonic_from_slice<ErrorCanonic>(
        enc: &[u8],
        r: &RelativeTo,
    ) -> Result<Self, DecodeError<(), Infallible, ErrorCanonic>>
    where
        ErrorCanonic: From<ErrorReason>,
        Self: RelativeDecodableCanonic<RelativeTo, ErrorReason, ErrorCanonic>,
    {
        pollster::block_on(Self::relative_decode_canonic_from_slice(enc, r))
    }
}

impl<T> RelativeDecodableSync<(), T::ErrorReason> for T where T: DecodableSync {}
