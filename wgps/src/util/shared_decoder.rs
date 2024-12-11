use std::{convert::Infallible, marker::PhantomData, ops::DerefMut};

use either::Either::*;
use ufotofu::{BulkProducer, Producer};

use wb_async_utils::Mutex;
use ufotofu_codec::{Decodable, DecodeError};

/// A `Producer` that decodes values of type `T` which were produced by a `BulkProducer` of `u8`s. The inner `BulkProducer` may be shared between several parts of the codebase via a `Mutex`, this type guarantees uninterrupted access to all bytes which encode a single value. Intended to be used with `T` being the type of some message of the WGPS.
pub(crate) struct SharedDecoder<'m, T, C> {
    pub inner: &'m Mutex<C>,
    phantom: PhantomData<T>,
}

impl<'m, T, C> SharedDecoder<'m, T, C> {
    /// Create a new `SharedDecoder` from the given `&'m Mutex<C>`.
    pub fn new(c: &'m Mutex<C>) -> Self {
        SharedDecoder {
            inner: c,
            phantom: PhantomData,
        }
    }
}

impl<'m, T, C> Producer for SharedDecoder<'m, T, C>
where
    T: Decodable,
    C: BulkProducer<Item = u8>,
{
    type Item = T;

    type Final = Infallible;

    type Error = DecodeError<C::Error>;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.inner.write().await;

        Ok(Left(T::decode_canonical(p.deref_mut()).await?))
    }
}
