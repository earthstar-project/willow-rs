//! Provides [`SharedProducer`], a way for creating multiple independent handles that coordinate termporary exclusive access to a shared underlying producer.

use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use either::Either::{self, *};

use ufotofu::{BufferedProducer, BulkProducer, Producer};

use crate::{mutex::WriteGuard, Mutex};

/// The state shared between all clones of the same [`SharedProducer`]. This is fully opaque, but we expose it to give control over where it is allocated.
pub struct State<P: Producer>(Mutex<MutexState<P>>);

impl<P: Producer> State<P> {
    /// Creates a new [`State`] for managing shared access to the same `producer`.
    pub fn new(producer: P) -> Self {
        State(Mutex::new(MutexState {
            p: producer,
            last: None,
        }))
    }
}

impl<P> Debug for State<P>
where
    P: Producer + Debug,
    P::Final: Debug,
    P::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("State").field(&self.0).finish()
    }
}

#[derive(Debug)]
struct MutexState<P: Producer> {
    p: P,
    last: Option<Result<P::Final, P::Error>>,
}

/// A producer adaptor that allows access to the same producer from multiple parts in the codebase by providing a cloneable handle.
///
/// This type provides two core pieces of functionality: ensuring exclusive access to the underlying producer so that independent components do not "steal" each other's data, and cloning and caching errors and final values to present them to all handles. More specifically:
///
/// A [`SharedProducer`] handle does not itself implement the producer traits. Instead, one must call the async [`access_producer`](SharedProducer::access_producer) method, which grants a [`SharedProducerAccess`] which implements the producer traits. If another [`SharedProducerAccess`] is currently alive, the method non-blocks until the inner producer can be accessed safely. Pending accesses are woken up in FIFO-order.
///
/// The `Final` and the `Error` type of the inner producer must implement [`Clone`]. Once the inner producer emits its final value or an error, all [`SharedProducerAccess`] handles will emit clones of that value. The implementation ensures that the inner producer is not used after an error or the final item.
///
/// The shared state between all clones of the same [`SharedProducer`] must be supplied via a reference of type `R` to an [opaque handle](State) at creation time; this gives control over how to allocate the state and manage its lifetime to the user. Typical choices for `R` would be an `Rc<shared_producer::State>` or a `&shared_producer::State`.
///
/// ```
/// use core::time::Duration;
/// use either::Either::*;
/// use wb_async_utils::shared_producer::*;
/// use smol::{Timer, block_on};
/// use ufotofu::{Producer, producer::{TestProducer, TestProducerBuilder}};
///
/// let underlying_p: TestProducer<u8, (), i16> = TestProducerBuilder::new(vec![1, 2, 3].into(), Err(-17)).build();
/// let state = State::new(underlying_p);
///
/// let shared1 = SharedProducer::new(&state);
/// let shared2 = shared1.clone();
///
/// let read_some_items1 = async {
///     {
///         let mut p_handle = shared1.access_producer().await;
///         Timer::after(Duration::from_millis(50)).await; // Since we hold a handle right now, obtaining the second handle has to wait for us.
///         assert_eq!(Ok(Left(1)), p_handle.produce().await);
///     }
///
///     Timer::after(Duration::from_millis(50)).await; // Having dropped p_handle, the other task can jump in now.
///
///     {
///         let mut p_handle = shared1.access_producer().await;
///         assert_eq!(Ok(Left(3)), p_handle.produce().await);
///         assert_eq!(Err(-17), p_handle.produce().await);
///     }
/// };
///
/// let read_some_items2 = async {
///     {
///         let mut p_handle = shared2.access_producer().await;
///         assert_eq!(Ok(Left(2)), p_handle.produce().await);
///     }
///
///     Timer::after(Duration::from_millis(50)).await;
///
///     let mut p_handle = shared2.access_producer().await;
///     assert_eq!(Err(-17), p_handle.produce().await); // Replays a cached `-17` instead of using the underlying producer.
/// };
///
/// smol::block_on(futures::future::join(read_some_items1, read_some_items2));
/// ```
#[derive(Debug, Clone)]
pub struct SharedProducer<R, P>
where
    P: Producer,
    R: Deref<Target = State<P>> + Clone,
{
    state_ref: R,
}

impl<R, P> SharedProducer<R, P>
where
    P: Producer,
    R: Deref<Target = State<P>> + Clone,
{
    /// Creates a new `SharedProducer` from a cloneable reference to a [`State`].
    pub fn new(state_ref: R) -> Self {
        Self { state_ref }
    }

    /// Obtains exclusive access to the underlying producer, waiting if necessary.
    pub async fn access_producer(&self) -> SharedProducerAccess<P> {
        SharedProducerAccess(self.state_ref.deref().0.write().await)
    }
}

/// A handle that represents exclusive access to an underlying shared producer. Implements the producer traits and forwards method calls to the underlying producer. After the underlying producer has emitted its final item or an error, a [`SharedProducerAccess`] replays copies of that last value instead of continuing to call methods on the underlying producer.
pub struct SharedProducerAccess<'shared_producer, P: Producer>(
    WriteGuard<'shared_producer, MutexState<P>>,
);

impl<P> Debug for SharedProducerAccess<'_, P>
where
    P: Producer + Debug,
    P::Final: Debug,
    P::Error: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedProducerAccess")
            .field(&self.0)
            .finish()
    }
}

impl<P> Producer for SharedProducerAccess<'_, P>
where
    P: Producer,
    P::Final: Clone,
    P::Error: Clone,
{
    type Item = P::Item;

    type Final = P::Final;

    type Error = P::Error;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let inner_state = self.0.deref_mut();

        match inner_state.last.as_ref() {
            Some(Ok(fin)) => Ok(Right(fin.clone())),
            Some(Err(err)) => Err(err.clone()),
            None => match inner_state.p.produce().await {
                Ok(Left(item)) => Ok(Left(item)),
                Ok(Right(fin)) => {
                    inner_state.last = Some(Ok(fin.clone()));
                    Ok(Right(fin))
                }
                Err(err) => {
                    inner_state.last = Some(Err(err.clone()));
                    Err(err)
                }
            },
        }
    }
}

impl<P> BufferedProducer for SharedProducerAccess<'_, P>
where
    P: BufferedProducer,
    P::Final: Clone,
    P::Error: Clone,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let inner_state = self.0.deref_mut();

        match inner_state.last.as_ref() {
            Some(Ok(_fin)) => Ok(()), // Slurping becomes a no-op after the final value has been emitted on a different handle.
            Some(Err(err)) => Err(err.clone()),
            None => match inner_state.p.slurp().await {
                Ok(()) => Ok(()),
                Err(err) => {
                    inner_state.last = Some(Err(err.clone()));
                    Err(err)
                }
            },
        }
    }
}

impl<P> BulkProducer for SharedProducerAccess<'_, P>
where
    P: BulkProducer,
    P::Final: Clone,
    P::Error: Clone,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        let inner_state = self.0.deref_mut();

        match inner_state.last.as_ref() {
            Some(Ok(fin)) => Ok(Right(fin.clone())),
            Some(Err(err)) => Err(err.clone()),
            None => match inner_state.p.expose_items().await {
                Ok(Left(items)) => Ok(Left(items)),
                Ok(Right(fin)) => {
                    inner_state.last = Some(Ok(fin.clone()));
                    Ok(Right(fin))
                }
                Err(err) => {
                    inner_state.last = Some(Err(err.clone()));
                    Err(err)
                }
            },
        }
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        let inner_state = self.0.deref_mut();

        match inner_state.last.as_ref() {
            Some(Ok(_fin)) => Ok(()), // Consider_produced becomes a no-op after the final value has been emitted on a different handle.
            Some(Err(err)) => Err(err.clone()),
            None => match inner_state.p.consider_produced(amount).await {
                Ok(()) => Ok(()),
                Err(err) => {
                    inner_state.last = Some(Err(err.clone()));
                    Err(err)
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;
    use smol::{block_on, Timer};
    use ufotofu::{
        producer::{TestProducer, TestProducerBuilder},
        Producer,
    };

    #[test]
    fn test_shared_producer() {
        let underlying_p: TestProducer<u8, (), i16> =
            TestProducerBuilder::new(vec![1, 2, 3].into(), Err(-17)).build();
        let state = State::new(underlying_p);

        let shared1 = SharedProducer::new(&state);
        let shared2 = shared1.clone();

        let read_some_items1 = async {
            {
                let mut p_handle = shared1.access_producer().await;
                Timer::after(Duration::from_millis(50)).await; // Since we hold a handle right now, obtaining the second handle has to wait for us.
                assert_eq!(Ok(Left(1)), p_handle.produce().await);
            }

            Timer::after(Duration::from_millis(50)).await; // Having dropped p_handle, the other task can jump in now.

            {
                let mut p_handle = shared1.access_producer().await;
                assert_eq!(Ok(Left(3)), p_handle.produce().await);
                assert_eq!(Err(-17), p_handle.produce().await);
            }
        };

        let read_some_items2 = async {
            Timer::after(Duration::from_millis(10)).await; // Ensure that the other task "starts".

            {
                let mut p_handle = shared2.access_producer().await;
                assert_eq!(Ok(Left(2)), p_handle.produce().await);
            }

            Timer::after(Duration::from_millis(50)).await;

            let mut p_handle = shared2.access_producer().await;
            assert_eq!(Err(-17), p_handle.produce().await); // Replays a cached `-17` instead of using the underlying producer.
        };

        block_on(futures::future::join(read_some_items1, read_some_items2));
    }
}
