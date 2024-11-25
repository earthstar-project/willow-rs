use async_cell::unsync::AsyncCell;
use ufotofu::local_nb::{
    BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer,
};

/// A mutex for single-threaded non-blocking code: you can `.await` exclusive access to the wrapped value.
pub struct NbMutex<T>(AsyncCell<T>);

impl<T> NbMutex<T> {
    /// Wraps a given consumer.
    pub fn new(consumer: T) -> Self {
        NbMutex(AsyncCell::new_with(consumer))
    }

    /// Receive access to the wrapped consumer, waiting if necessary while another program component has access.
    pub async fn access<'m>(&'m self) -> ExclusiveAccess<'m, T> {
        ExclusiveAccess {
            taken: Some(self.0.take().await),
            shared: self,
        }
    }
}

/// Exclusive access to the value stored in an [`NbMutex`] of lifetime `'m`.
///
/// The wrapped value is accessible via the implementations of `AsRef` and `AsMut`.
pub struct ExclusiveAccess<'m, T> {
    taken: Option<T>,
    shared: &'m NbMutex<T>,
}

impl<'m, T> Drop for ExclusiveAccess<'m, T> {
    fn drop(&mut self) {
        let taken = unsafe { self.taken.take().unwrap_unchecked() }; // We never set it to None outside of this function.
        self.shared.0.set(taken);
    }
}

impl<'m, T> AsRef<T> for ExclusiveAccess<'m, T> {
    fn as_ref(&self) -> &T {
        unsafe { self.taken.as_ref().unwrap_unchecked() }
    }
}

impl<'m, T> AsMut<T> for ExclusiveAccess<'m, T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.taken.as_mut().unwrap_unchecked() }
    }
}

// If extracting this ito a crate, the following impls should be put behind a `ufotofu` feature.

impl<'m, T> Consumer for ExclusiveAccess<'m, T>
where
    T: Consumer,
{
    type Item = T::Item;
    type Final = T::Final;
    type Error = T::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        Ok(self.as_mut().consume(item).await?)
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        Ok(self.as_mut().close(fin).await?)
    }
}

impl<'m, T> BufferedConsumer for ExclusiveAccess<'m, T>
where
    T: BufferedConsumer,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(self.as_mut().flush().await?)
    }
}

impl<'m, T> BulkConsumer for ExclusiveAccess<'m, T>
where
    T: BulkConsumer,
    T::Item: Copy,
{
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        Ok(self.as_mut().expose_slots().await?)
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        Ok(self.as_mut().consume_slots(amount).await?)
    }
}

impl<'m, T> Producer for ExclusiveAccess<'m, T>
where
    T: Producer,
{
    type Item = T::Item;
    type Final = T::Final;
    type Error = T::Error;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        Ok(self.as_mut().produce().await?)
    }
}

impl<'m, T> BufferedProducer for ExclusiveAccess<'m, T>
where
    T: BufferedProducer,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(self.as_mut().slurp().await?)
    }
}

impl<'m, T> BulkProducer for ExclusiveAccess<'m, T>
where
    T: BulkProducer,
    T::Item: Copy,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<either::Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        Ok(self.as_mut().expose_items().await?)
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        Ok(self.as_mut().consider_produced(amount).await?)
    }
}
