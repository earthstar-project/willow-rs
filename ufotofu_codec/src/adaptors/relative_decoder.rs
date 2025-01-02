use core::{borrow::Borrow, marker::PhantomData};

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use either::Either::{self, *};

use crate::{DecodeError, RelativeDecodable, RelativeDecodableCanonic};

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that decodes relative to some value of type `RelativeTo`.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`RelativeDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct RelativeDecoder<P, T, R, RelativeTo, ErrorReason> {
    inner: P,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo, ErrorReason)>,
}

impl<P, T, R, RelativeTo, ErrorReason> RelativeDecoder<P, T, R, RelativeTo, ErrorReason> {
    /// Creates a new [`RelativeDecoder`], decoding from the given `producer`. Decodes relative to `relative_to.borrow()`.
    pub fn new(producer: P, relative_to: R) -> Self {
        Self {
            inner: producer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer.
    pub fn into_inner(self) -> P {
        self.inner
    }

    /// Changes the value relative to which this decodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<P, T, R, RelativeTo, ErrorReason> Producer
    for RelativeDecoder<P, T, R, RelativeTo, ErrorReason>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodable<RelativeTo, ErrorReason>,
    R: Borrow<RelativeTo>,
{
    type Item = T;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, ErrorReason>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if let Right(fin) = self.inner.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(
            T::relative_decode(&mut self.inner, self.relative_to.borrow()).await?,
        ))
    }
}

impl<P, T, R, RelativeTo, ErrorReason> BufferedProducer
    for RelativeDecoder<P, T, R, RelativeTo, ErrorReason>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodable<RelativeTo, ErrorReason>,
    R: Borrow<RelativeTo>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}

/// Turns a [BulkProducer] of bytes into a [BufferedProducer] of `T`s that canonically decodes relative to some value of type `RelativeTo`.
///
/// Emits the underlying `Final` item when the underlying producer emits it before the first byte of an encoding. When the underlying producer emits its `Final` item in the middle of an encoding, the [`RelativeCanonicDecoder`] yields a [`DecodeError::UnexpectedEndOfInput`].
pub struct RelativeCanonicDecoder<P, T, R, RelativeTo, ErrorReason, ErrorCanonic> {
    inner: P,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo, ErrorReason, ErrorCanonic)>,
}

impl<P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
    RelativeCanonicDecoder<P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
{
    /// Creates a new [`RelativeCanonicDecoder`], canonically decoding from the given `producer`. Decodes relative to `relative_to.borrow()`.
    pub fn new(producer: P, relative_to: R) -> Self {
        Self {
            inner: producer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped producer.
    pub fn into_inner(self) -> P {
        self.inner
    }

    /// Changes the value relative to which this decodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<P, T, R, RelativeTo, ErrorReason, ErrorCanonic> Producer
    for RelativeCanonicDecoder<P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
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
        if let Right(fin) = self.inner.expose_items().await? {
            return Ok(Right(fin));
        }

        Ok(Left(
            T::relative_decode_canonic(&mut self.inner, self.relative_to.borrow()).await?,
        ))
    }
}

impl<P, T, R, RelativeTo, ErrorReason, ErrorCanonic> BufferedProducer
    for RelativeCanonicDecoder<P, T, R, RelativeTo, ErrorReason, ErrorCanonic>
where
    P: BulkProducer<Item = u8>,
    T: RelativeDecodableCanonic<RelativeTo, ErrorReason, ErrorCanonic>,
    R: Borrow<RelativeTo>,
    ErrorCanonic: From<ErrorReason>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.inner.slurp().await?)
    }
}
