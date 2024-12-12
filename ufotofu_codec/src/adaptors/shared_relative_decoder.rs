use core::{borrow::Borrow, marker::PhantomData, ops::DerefMut};

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use either::Either::{self, *};
use wb_async_utils::Mutex;

use crate::{DecodeError, RelativeDecodable, RelativeDecodableCanonic};

/// Turns a reference to a [`Mutex`] of a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that decodes relative to some value of type `RelativeTo`. Takes exclusive mutable access for the duration of decoding every individual item, thus ensuring that no other reference to the same producer steals away part of the input data.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`SharedRelativeDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct SharedRelativeDecoder<'mutex, P, T, R, RelativeTo, ErrorReason> {
    inner: &'mutex Mutex<P>,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo, ErrorReason)>,
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason>
    SharedRelativeDecoder<'mutex, P, T, R, RelativeTo, ErrorReason>
{
    /// Creates a new [`SharedRelativeDecoder`], decoding from the given `producer`. Decodes relative to `relative_to.borrow()`.
    pub fn new(producer: &'mutex Mutex<P>, relative_to: R) -> Self {
        Self {
            inner: producer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer reference.
    pub fn into_inner(self) -> &'mutex Mutex<P> {
        self.inner
    }

    /// Changes the value relative to which this decodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason> Producer
    for SharedRelativeDecoder<'mutex, P, T, R, RelativeTo, ErrorReason>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodable<RelativeTo, ErrorReason>,
    R: Borrow<RelativeTo>,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, ErrorReason>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.inner.write().await;

        if let Right(fin) = p.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(
            T::relative_decode(p.deref_mut(), self.relative_to.borrow()).await?,
        ))
    }
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason> BufferedProducer
    for SharedRelativeDecoder<'mutex, P, T, R, RelativeTo, ErrorReason>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodable<RelativeTo, ErrorReason>,
    R: Borrow<RelativeTo>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let mut p = self.inner.write().await;
        Ok(p.slurp().await?)
    }
}

/// Turns a reference to a [`Mutex`] of a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that canonically decodes relative to some value of type `RelativeTo`. Takes exclusive mutable access for the duration of decoding every individual item, thus ensuring that no other reference to the same producer steals away part of the input data.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`SharedRelativeCanonicDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct SharedRelativeCanonicDecoder<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic> {
    inner: &'mutex Mutex<P>,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo, ErrorReason, ErrorCanonic)>,
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
    SharedRelativeCanonicDecoder<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
{
    /// Creates a new [`SharedRelativeCanonicDecoder`], canonically decoding from the given `producer`. Decodes relative to `relative_to.borrow()`.
    pub fn new(producer: &'mutex Mutex<P>, relative_to: R) -> Self {
        Self {
            inner: producer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer reference.
    pub fn into_inner(self) -> &'mutex Mutex<P> {
        self.inner
    }

    /// Changes the value relative to which this decodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic> Producer
    for SharedRelativeCanonicDecoder<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodableCanonic<RelativeTo, ErrorReason, ErrorCanonic>,
    R: Borrow<RelativeTo>,
    ErrorCanonic: From<ErrorReason>,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, ErrorCanonic>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.inner.write().await;

        if let Right(fin) = p.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(
            T::relative_decode_canonic(p.deref_mut(), self.relative_to.borrow()).await?,
        ))
    }
}

impl<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic> BufferedProducer
    for SharedRelativeCanonicDecoder<'mutex, P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodableCanonic<RelativeTo, ErrorReason, ErrorCanonic>,
    R: Borrow<RelativeTo>,
    ErrorCanonic: From<ErrorReason>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let mut p = self.inner.write().await;
        Ok(p.slurp().await?)
    }
}
