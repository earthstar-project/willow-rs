use core::{marker::PhantomData, ops::DerefMut};

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use either::Either::{self, *};

use wb_async_utils::Mutex;

use crate::{Decodable, DecodableCanonic, DecodeError};

/// Turns a reference to a [`Mutex`] of a [BulkProducer] of bytes into a [BufferedProducer] of `T`s. Takes exclusive mutable access for the duration of decoding every individual item, thus ensuring that no other reference to the same producer steals away part of the input data.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`SharedDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct SharedDecoder<'mutex, P, T> {
    inner: &'mutex Mutex<P>,
    phantom: PhantomData<T>,
}

impl<'mutex, P, T> SharedDecoder<'mutex, P, T> {
    /// Creates a new [`SharedDecoder`], decoding from the given `producer`.
    pub fn new(producer: &'mutex Mutex<P>) -> Self {
        Self {
            inner: producer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer reference.
    pub fn into_inner(self) -> &'mutex Mutex<P> {
        self.inner
    }
}

impl<P, T> Producer for SharedDecoder<'_, P, T>
where
    P: BulkProducer<Item = u8>,
    T: Decodable,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, T::ErrorReason>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.inner.write().await;

        if let Right(fin) = p.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(T::decode(p.deref_mut()).await?))
    }
}

impl<P, T> BufferedProducer for SharedDecoder<'_, P, T>
where
    P: BulkProducer<Item = u8>,
    T: Decodable,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let mut p = self.inner.write().await;
        Ok(p.slurp().await?)
    }
}

/// Turns a reference to a [`Mutex`] of a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that errors if an encoding is not canonic. Takes exclusive mutable access for the duration of decoding every individual item, thus ensuring that no other reference to the same producer steals away part of the input data.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`SharedCanonicDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct SharedCanonicDecoder<'mutex, P, T> {
    inner: &'mutex Mutex<P>,
    phantom: PhantomData<T>,
}

impl<'mutex, P, T> SharedCanonicDecoder<'mutex, P, T> {
    /// Creates a new [`SharedCanonicDecoder`], canonically decoding from the given `producer`.
    pub fn new(producer: &'mutex Mutex<P>) -> Self {
        Self {
            inner: producer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer.
    pub fn into_inner(self) -> &'mutex Mutex<P> {
        self.inner
    }
}

impl<P, T> Producer for SharedCanonicDecoder<'_, P, T>
where
    P: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, T::ErrorCanonic>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.inner.write().await;

        if let Right(fin) = p.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(T::decode_canonic(p.deref_mut()).await?))
    }
}

impl<P, T> BufferedProducer for SharedCanonicDecoder<'_, P, T>
where
    P: BulkProducer<Item = u8>,
    T: DecodableCanonic,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let mut p = self.inner.write().await;
        Ok(p.slurp().await?)
    }
}
