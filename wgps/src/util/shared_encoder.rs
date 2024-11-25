//! In implementing the WGPS, we want to write to the same underlying transport channel from several independent places in the codebase. This module provides a struct for enabling this: a shared reference to a `Consumer`, and anyone with such a reference can request exclusive access to that Consumer for any amount of time. If another entity accesses the consumer, then the request non-blocks until it becomes available.

use std::marker::PhantomData;

use ufotofu::local_nb::{BulkConsumer, Consumer};

use willow_encoding::Encodable;

use super::NbMutex;

/// A `Consumer` that encodes the values of type `T` which it consumes into a `BulkConsumer` of `u8`s. The inner `BulkConsumer` may be shared between several parts of the codebase via an `NbMutex`, this type guarantees that all bytes for a single consumed item will be consecutively fed into the underlying BulkConsumer. Intended to be used with `T` being the type of some message of the WGPS.
pub(crate) struct SharedEncoder<'m, T, C> {
    pub inner: &'m NbMutex<C>,
    phantom: PhantomData<T>,
}

impl<'m, T, C> SharedEncoder<'m, T, C> {
    /// Create a new `SharedEncoder` from the given `&'m NbMutex<C>`.
    pub fn new(c: &'m NbMutex<C>) -> Self {
        SharedEncoder {
            inner: c,
            phantom: PhantomData,
        }
    }
}

impl<'m, T, C> Consumer for SharedEncoder<'m, T, C>
where
    T: Encodable,
    C: BulkConsumer<Item = u8>,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        let mut c = self.inner.access().await;
        Ok(item.encode(&mut c).await?)
    }

    /// Does nothing. Whoever holds the actual NbMutex is responsible for closing the consumer.
    async fn close(&mut self, _fin: Self::Final) -> Result<(), Self::Error> {
        Ok(())
    }
}
