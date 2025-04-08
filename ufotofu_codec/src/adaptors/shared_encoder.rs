use core::{marker::PhantomData, ops::DerefMut};

use ufotofu::{BufferedConsumer, BulkConsumer, Consumer};

use wb_async_utils::Mutex;

use crate::Encodable;

/// Turns a reference to a [`Mutex`] of a [BulkConsumer] of bytes into a [BufferedConsumer] of `T`s. Takes exclusive mutable access for the duration of encoding every individual item, thus ensuring that the encoding is not interleaved with other data.
///
/// Closing this does nothing. Whoever holds the actual Mutex is responsible for closing the underlying consumer.
pub struct SharedEncoder<'mutex, C, T> {
    inner: &'mutex Mutex<C>,
    phantom: PhantomData<T>,
}

impl<'mutex, C, T> SharedEncoder<'mutex, C, T> {
    /// Creates a new [`SharedEncoder`], encoding into the given `consumer`.
    pub fn new(consumer: &'mutex Mutex<C>) -> Self {
        Self {
            inner: consumer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped consumer reference.
    pub fn into_inner(self) -> &'mutex Mutex<C> {
        self.inner
    }
}

impl<C, T> Consumer for SharedEncoder<'_, C, T>
where
    C: BulkConsumer<Item = u8>,
    T: Encodable,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        let mut c = self.inner.write().await;
        item.encode(c.deref_mut()).await
    }

    /// Does nothing. Whoever holds the actual [`Mutex`] is responsible for closing the underlying consumer.
    async fn close(&mut self, _fin: Self::Final) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<C, T> BufferedConsumer for SharedEncoder<'_, C, T>
where
    C: BulkConsumer<Item = u8>,
    T: Encodable,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        let mut c = self.inner.write().await;
        c.flush().await
    }
}
