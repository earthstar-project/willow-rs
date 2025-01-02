use core::marker::PhantomData;

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use either::Either::{self, *};

use crate::{Decodable, DecodableCanonic, DecodeError};

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`Decoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct Decoder<P, T> {
    inner: P,
    phantom: PhantomData<T>,
}

impl<P, T> Decoder<P, T> {
    /// Creates a new [`Decoder`], decoding from the given `producer`.
    pub fn new(producer: P) -> Self {
        Self {
            inner: producer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer.
    pub fn into_inner(self) -> P {
        self.inner
    }
}

impl<P, T> Producer for Decoder<P, T>
where
    P: BulkProducer<Item = u8>,
    T: Decodable,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, T::ErrorReason>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if let Right(fin) = self.inner.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(T::decode(&mut self.inner).await?))
    }
}

impl<P, T> BufferedProducer for Decoder<P, T>
where
    P: BulkProducer<Item = u8>,
    T: Decodable,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that errors if an encoding it decodes is not canonic.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`CanonicDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct CanonicDecoder<P, T> {
    inner: P,
    phantom: PhantomData<T>,
}

impl<P, T> CanonicDecoder<P, T> {
    /// Creates a new [`CanonicDecoder`], canonically decoding from the given `producer`.
    pub fn new(producer: P) -> Self {
        Self {
            inner: producer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer.
    pub fn into_inner(self) -> P {
        self.inner
    }
}

impl<P, T> Producer for CanonicDecoder<P, T>
where
    P: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, T::ErrorCanonic>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if let Right(fin) = self.inner.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(T::decode_canonic(&mut self.inner).await?))
    }
}

impl<P, T> BufferedProducer for CanonicDecoder<P, T>
where
    P: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}
