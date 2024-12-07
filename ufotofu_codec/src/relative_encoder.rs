use core::{borrow::Borrow, marker::PhantomData};

use ufotofu::{BufferedConsumer, BulkConsumer, Consumer};

use crate::RelativeEncodable;

/// Turns a [BulkConsumer] of bytes into a [BufferedConsumer] of `T`s, which are encoded relative to some value of type `RelativeTo`.
pub struct RelativeEncoder<C, T, R, RelativeTo> {
    inner: C,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo)>,
}

impl<C, T, R, RelativeTo> RelativeEncoder<C, T, R, RelativeTo> {
    /// Creates a new [`Encoder`], relatively encoding into the given `consumer`. Encodes relative to `relative_to.borrow()`.
    pub fn new(consumer: C, relative_to: R) -> Self {
        Self {
            inner: consumer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped consumer.
    pub fn into_inner(self) -> C {
        self.inner
    }

    /// Changes the value relative to which this encodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<C, T, R, RelativeTo> Consumer for RelativeEncoder<C, T, R, RelativeTo>
where
    C: BulkConsumer<Item = u8>,
    T: RelativeEncodable<RelativeTo>,
    R: Borrow<RelativeTo>,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        item.relative_encode(&mut self.inner, self.relative_to.borrow())
            .await
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        self.inner.close(fin).await
    }
}

impl<C, T, R, RelativeTo> BufferedConsumer for RelativeEncoder<C, T, R, RelativeTo>
where
    C: BulkConsumer<Item = u8>,
    T: RelativeEncodable<RelativeTo>,
    R: Borrow<RelativeTo>,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().await
    }
}
