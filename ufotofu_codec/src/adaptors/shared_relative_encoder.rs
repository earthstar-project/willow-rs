use core::{borrow::Borrow, marker::PhantomData, ops::DerefMut};

use ufotofu::{BufferedConsumer, BulkConsumer, Consumer};
use wb_async_utils::Mutex;

use crate::RelativeEncodable;

/// Turns a reference to a [`Mutex`] of a [BulkConsumer] of bytes into a [BufferedConsumer] of `T`s, which are encoded relative to some value of type `RelativeTo`. Takes exclusive mutable access for the duration of encoding every individual item, thus ensuring that the encoding is not interleaved with other data.
///
/// Closing this does nothing. Whoever holds the actual Mutex is responsible for closing the underlying consumer.
pub struct SharedRelativeEncoder<'mutex, C, T, R, RelativeTo> {
    inner: &'mutex Mutex<C>,
    relative_to: R,
    phantom: PhantomData<(T, RelativeTo)>,
}

impl<'mutex, C, T, R, RelativeTo> SharedRelativeEncoder<'mutex, C, T, R, RelativeTo> {
    /// Creates a new [`SharedRelativeEncoder`], relatively encoding into the given `consumer`. Encodes relative to `relative_to.borrow()`.
    pub fn new(consumer: &'mutex Mutex<C>, relative_to: R) -> Self {
        Self {
            inner: consumer,
            relative_to,
            phantom: PhantomData,
        }
    }

    /// Consumes `self` and returns ownership of the wrapped consumer reference.
    pub fn into_inner(self) -> &'mutex Mutex<C> {
        self.inner
    }

    /// Changes the value relative to which this encodes.
    pub fn set_relative_to(&mut self, relative_to: R) {
        self.relative_to = relative_to;
    }
}

impl<'mutex, C, T, R, RelativeTo> Consumer for SharedRelativeEncoder<'mutex, C, T, R, RelativeTo>
where
    C: BulkConsumer<Item = u8>,
    T: RelativeEncodable<RelativeTo>,
    R: Borrow<RelativeTo>,
{
    type Item = T;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        let mut c = self.inner.write().await;
        item.relative_encode(c.deref_mut(), self.relative_to.borrow())
            .await
    }

    /// Does nothing. Whoever holds the actual [`Mutex`] is responsible for closing the underlying consumer.
    async fn close(&mut self, _fin: Self::Final) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<'mutex, C, T, R, RelativeTo> BufferedConsumer
    for SharedRelativeEncoder<'mutex, C, T, R, RelativeTo>
where
    C: BulkConsumer<Item = u8>,
    T: RelativeEncodable<RelativeTo>,
    R: Borrow<RelativeTo>,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        let mut c = self.inner.write().await;
        c.flush().await
    }
}
