use core::{convert::Infallible, marker::PhantomData};

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use either::Either::*;

use crate::{Decodable, DecodableCanonic, DecodeError};

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s.
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

impl<C, T> Producer for Decoder<C, T>
where
    C: BulkProducer<Item = u8>,
    T: Decodable,
{
    type Item = T;

    type Final = Infallible;

    type Error = DecodeError<C::Final, C::Error, T::ErrorReason>;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        Ok(Left(T::decode(&mut self.inner).await?))
    }
}

impl<C, T> BufferedProducer for Decoder<C, T>
where
    C: BulkProducer<Item = u8>,
    T: Decodable,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that errors if an encoding it decodes is not canonic.
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

impl<C, T> Producer for CanonicDecoder<C, T>
where
    C: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    type Item = T;

    type Final = Infallible;

    type Error = DecodeError<C::Final, C::Error, T::ErrorCanonic>;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        Ok(Left(T::decode_canonic(&mut self.inner).await?))
    }
}

impl<C, T> BufferedProducer for CanonicDecoder<C, T>
where
    C: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}
