use core::marker::PhantomData;

use ufotofu::{BufferedConsumer, BulkConsumer, Consumer};

use crate::Encodable;

/// Turns a [BulkConsumer] of bytes into a [BufferedConsumer] of `T`s.
pub struct Encoder<C, T> {
    inner: C,
    phantom: PhantomData<T>,
}

impl<C, T> Encoder<C, T> {
    /// Creates a new [`Encoder`], encoding into the given `consumer`.
    pub fn new(consumer: C) -> Self {
        Self {
            inner: consumer,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped consumer.
    pub fn into_inner(self) -> C {
        self.inner
    }
}

impl<C, T> Consumer for Encoder<C, T>
where
    C: BulkConsumer<Item = u8>,
    T: Encodable,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        item.encode(&mut self.inner).await
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        self.inner.close(fin).await
    }
}

impl<C, T> BufferedConsumer for Encoder<C, T>
where
    C: BulkConsumer<Item = u8>,
    T: Encodable,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().await
    }
}
