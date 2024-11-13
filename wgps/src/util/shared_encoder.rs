//! In implementing the WGPS, we want to write to the same underlying transport channel from several independent places in the codebase. This module provides a struct for enabling this: a shared reference to a `Consumer`, and anyone with such a reference can request exclusive access to that Consumer for any amount of time. If another entity accesses the consumer, then the request non-blocks until it becomes available.

use std::{marker::PhantomData, rc::Rc};

use async_cell::unsync::AsyncCell;

use ufotofu::local_nb::{BulkConsumer, Consumer};

use willow_encoding::Encodable;

/// An `unsync::AsyncCell` inside an `Rc`.
/// We made this a type alias to allow for easier refactoring into a `sync::AsyncCell` inside an `Arc`.
pub type AsyncShared<T> = Rc<AsyncCell<T>>;

/// A `Consumer` that encodes the values of type `T` which it consumes into a `BulkConsumer` of `u8`s. The inner `BulkConsumer` may be shared between several parts of the codebase via an `AsyncCell`, this type guarantees that all bytes for a single consumed item will be consecutively fed into the underlying BulkConsumer. Intended to be used with `T` being the type of some message of the WGPS.
pub(crate) struct SharedEncoder<T, C> {
    pub inner: AsyncShared<C>,
    phantom: PhantomData<T>,
}

impl<T, C> SharedEncoder<T, C> {
    /// Create a new `SharedEncoder` from the given `AsyncShared` consumer.
    pub fn new(c: AsyncShared<C>) -> Self {
        SharedEncoder {
            inner: c,
            phantom: PhantomData,
        }
    }
}

impl<T, C> Consumer for SharedEncoder<T, C>
where
    T: Encodable,
    C: BulkConsumer<Item = u8>,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        let mut c = self.inner.take().await;
        item.encode(&mut c).await?;
        self.inner.set(c);
        Ok(())
    }

    /// If this holds the only remaining reference to the `AsyncShared<C>` from which this was constructed, close the inner BulkConsumer. Otherwise, do nothing.
    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        if Rc::strong_count(&self.inner) == 1 {
            let mut c = self.inner.take().await;
            c.close(fin).await?;
            self.inner.set(c);
        } else {
            // do nothing
        }

        return Ok(());
    }
}
