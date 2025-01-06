use core::{
    cell::Cell,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

use fairly_unsafe_cell::*;

use crate::{extend_lifetime, extend_lifetime_mut};

/// An awaitable, single-threaded mutex. Only a single reference to the contents of the mutex can exist at any time.
///
/// All accesses are parked and waked in FIFO order.
pub struct Mutex<T> {
    value: FairlyUnsafeCell<T>,
    currently_used: Cell<bool>,
    parked: FairlyUnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> Mutex<T> {
    /// Creates a new mutex storing the given value.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    ///
    /// let m = Mutex::new(5);
    /// assert_eq!(5, m.into_inner());
    /// ```
    pub fn new(value: T) -> Self {
        Mutex {
            value: FairlyUnsafeCell::new(value),
            currently_used: Cell::new(false),
            parked: FairlyUnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the mutex and returns the wrapped value.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    ///
    /// let m = Mutex::new(5);
    /// assert_eq!(5, m.into_inner());
    /// ```
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Gives read access to the wrapped value, waiting if necessary.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     let handle = m.read().await;
    ///     assert_eq!(0, *handle.deref());
    /// });
    /// ```
    pub async fn read(&self) -> ReadGuard<T> {
        ReadFuture(self).await
    }

    /// Gives read access if doing so is possible without waiting, returns `None` otherwise.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let m = Mutex::new(0);
    /// assert_eq!(&0, m.try_read().unwrap().deref());
    ///
    /// block_on(futures::future::join(async {
    ///     let handle = m.read().await;
    ///     Timer::after(Duration::from_millis(50)).await;
    /// }, async {
    ///     assert!(m.try_read().is_none());
    /// }));
    /// ```
    pub fn try_read(&self) -> Option<ReadGuard<T>> {
        if self.currently_used.get() {
            return None;
        }

        self.currently_used.set(true);
        Some(ReadGuard { mutex: self })
    }

    /// Gives write access to the wrapped value, waiting if necessary.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::{Deref, DerefMut};
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let m = Mutex::new(0);
    /// block_on(futures::future::join(async {
    ///     let mut handle = m.write().await;
    ///     // Simulate doing some work.
    ///     Timer::after(Duration::from_millis(50)).await;
    ///     assert_eq!(0, *handle.deref());
    ///     *handle.deref_mut() = 1;
    /// }, async {
    ///     // This future is "faster", but has to wait for the "work" performed by the first one.
    ///     let mut handle = m.write().await;
    ///     assert_eq!(1, *handle.deref());
    /// }));
    /// ```
    pub async fn write(&self) -> WriteGuard<T> {
        WriteFuture(self).await
    }

    /// Gives write access if doing so is possible without waiting, returns `None` otherwise.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let m = Mutex::new(0);
    /// assert_eq!(&0, m.try_write().unwrap().deref());
    ///
    /// block_on(futures::future::join(async {
    ///     let handle = m.write().await;
    ///     Timer::after(Duration::from_millis(50)).await;
    /// }, async {
    ///     assert!(m.try_write().is_none());
    /// }));
    /// ```
    pub fn try_write(&self) -> Option<WriteGuard<T>> {
        if self.currently_used.get() {
            return None;
        }

        self.currently_used.set(true);
        Some(WriteGuard { mutex: self })
    }

    /// Sets the wrapped value.
    /// Needs to `.await` write access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     m.set(1).await;
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn set(&self, to: T) {
        let mut guard = self.write().await;
        *guard = to;
    }

    /// Replaces the wrapped value, and returns the old one.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(0, m.replace(1).await);
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn replace(&self, mut to: T) -> T {
        let mut guard = self.write().await;
        core::mem::swap(guard.deref_mut(), &mut to);
        to
    }

    /// Updates the wrapped value with the given function.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     m.update(|x| x + 1).await;
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn update(&self, with: impl FnOnce(&T) -> T) {
        let mut guard = self.write().await;
        *guard = with(&guard);
    }

    /// Updates the wrapped value with the successful result of the given function, or propagates the error of the function.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(Ok(()), m.fallible_update(|x| Ok::<u8, i8>(x + 1)).await);
    ///     assert_eq!(1, *m.read().await.deref());
    ///
    ///     assert_eq!(Err(-17), m.fallible_update(|_| Err(-17)).await);
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn fallible_update<E>(&self, with: impl FnOnce(&T) -> Result<T, E>) -> Result<(), E> {
        let mut guard = self.write().await;
        *guard = with(&guard)?;
        Ok(())
    }

    // /// Updates the wrapped value with the given async function.
    // /// Needs to `.await` read access internally.
    // ///
    // /// ```
    // /// use wb_async_utils::mutex::*;
    // /// use core::ops::Deref;
    // ///
    // /// let m = Mutex::new(0);
    // /// pollster::block_on(async {
    // ///     m.update_async(move |x| {async move {x + 1}}).await;
    // ///     assert_eq!(1, *m.read().await.deref());
    // /// });
    // /// ```
    // pub async fn update_async<Fut: Future<Output = T>>(&self, with: impl FnOnce(&T) -> Fut) {
    //     let mut guard = self.write().await;
    //     *guard = with(&guard).await;
    // }

    // /// Updates the wrapped value with the successful result of the given async function, or propagates the error of the function.
    // /// Needs to `.await` read access internally.
    // pub async fn fallible_update_async<E, Fut: Future<Output = Result<T, E>>>(
    //     &self,
    //     with: impl FnOnce(&T) -> Fut,
    // ) -> Result<(), E> {
    //     let mut guard = self.write().await;
    //     *guard = with(&guard).await?;
    //     Ok(())
    // }

    /// Mutates the wrapped value with the given function.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     m.mutate(|x| *x += 1).await;
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn mutate(&self, with: impl FnOnce(&mut T)) {
        let mut guard = self.write().await;
        with(&mut guard)
    }

    /// Mutates the wrapped value with the given fallible function, propagates its error if any.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::mutex::*;
    /// use core::ops::Deref;
    ///
    /// let m = Mutex::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(Ok(()), m.fallible_mutate(|x| {
    ///         *x +=1 ;
    ///         Ok::<(), i8>(())
    ///     }).await);
    ///     assert_eq!(1, *m.read().await.deref());
    ///
    ///     assert_eq!(Err(-17), m.fallible_mutate(|_| Err(-17)).await);
    ///     assert_eq!(1, *m.read().await.deref());
    /// });
    /// ```
    pub async fn fallible_mutate<E>(
        &self,
        with: impl FnOnce(&mut T) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut guard = self.write().await;
        with(&mut guard)
    }

    // /// Mutates the wrapped value with the given async function.
    // /// Needs to `.await` read access internally.
    // pub async fn mutate_async<Fut: Future<Output = ()>>(&self, with: impl FnOnce(&mut T) -> Fut) {
    //     let mut guard = self.write().await;
    //     with(&mut guard).await
    // }

    // /// Mutates the wrapped value with the given fallible async function, propagates its error if any.
    // /// Needs to `.await` read access internally.
    // pub async fn fallible_mutate_async<E, Fut: Future<Output = Result<(), E>>>(
    //     &self,
    //     with: impl FnOnce(&mut T) -> Fut,
    // ) -> Result<(), E> {
    //     let mut guard = self.write().await;
    //     with(&mut guard).await
    // }

    fn wake_next(&self) {
        // Safe because self.parked is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let mut r = unsafe { self.parked.borrow_mut() };

        if let Some(waker) = r.deref_mut().pop_front() {
            waker.wake()
        }
    }

    fn park(&self, cx: &mut Context<'_>) {
        // Safe because self.parked is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let mut r = unsafe { self.parked.borrow_mut() };

        r.deref_mut().push_back(cx.waker().clone());
    }
}

impl<T> AsMut<T> for Mutex<T> {
    fn as_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("Mutex");
        match self.try_read() {
            Some(guard) => {
                d.field(&&*guard);
            }
            None => {
                d.field(&format_args!("<locked>"));
            }
        }
        d.finish()
    }
}

/// Read-only access to the value stored in a [`Mutex`].
///
/// The wrapped value is accessible via the implementation of `Deref`.
///
/// ```
/// use wb_async_utils::mutex::*;
/// use core::ops::Deref;
/// use core::time::Duration;
///
/// let m = Mutex::new(0);
/// pollster::block_on(async {
///     let handle = m.read().await;
///     assert_eq!(0, *handle.deref());
/// });
/// ```
#[derive(Debug)]
pub struct ReadGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>,
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.currently_used.set(false);
        self.mutex.wake_next();
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let borrowed = unsafe { self.mutex.value.borrow() }; // Safe because a ReadGuard can never live at the same time as another guard or a `&mut Mutex`.
                                                             // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                             // as `self`.
        unsafe { extend_lifetime(borrowed.deref()) }
    }
}

/// Read and write access access to the value stored in a [`Mutex`].
///
/// The wrapped value is accessible via the implementation of `Deref` and `DerefMut`.
///
/// ```
/// use wb_async_utils::mutex::*;
/// use core::ops::{Deref, DerefMut};
/// use core::time::Duration;
/// use smol::{block_on, Timer};
///
/// let m = Mutex::new(0);
/// block_on(futures::future::join(async {
///     let mut handle = m.write().await;
///     // Simulate doing some work.
///     Timer::after(Duration::from_millis(50)).await;
///     assert_eq!(0, *handle.deref());
///     *handle.deref_mut() = 1;
/// }, async {
///     // This future is "faster", but has to wait for the "work" performed by the first one.
///     let mut handle = m.read().await;
///     assert_eq!(1, *handle.deref());
/// }));
/// ```
#[derive(Debug)]
pub struct WriteGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>,
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.currently_used.set(false);
        self.mutex.wake_next();
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let borrowed = unsafe { self.mutex.value.borrow() }; // Safe because a WriteGuard can never live at the same time as another guard or a `&mut Mutex`.
                                                             // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                             // as `self`.
        unsafe { extend_lifetime(borrowed.deref()) }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        let mut borrowed = unsafe { self.mutex.value.borrow_mut() }; // Safe because a WriteGuard can never live at the same time as another or a `&mut Mutex`.
                                                                     // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                                     // as `self`.
        unsafe { extend_lifetime_mut(borrowed.deref_mut()) }
    }
}

struct ReadFuture<'mutex, T>(&'mutex Mutex<T>);

impl<'mutex, T> Future for ReadFuture<'mutex, T> {
    type Output = ReadGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_read() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.0.park(cx);
                Poll::Pending
            }
        }
    }
}

struct WriteFuture<'mutex, T>(&'mutex Mutex<T>);

impl<'mutex, T> Future for WriteFuture<'mutex, T> {
    type Output = WriteGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_write() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.0.park(cx);
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;
    use smol::{block_on, Timer};

    #[test]
    fn test_mutex_basic() {
        let m = Mutex::new(0);

        let set1 = async {
            {
                let mut handle = m.write().await;
                Timer::after(Duration::from_millis(50)).await; // Since we hold a handle right now, obtaining the second handle has to wait for us.
                assert_eq!(0, *handle.deref());
                *handle.deref_mut() = 1;
            }
        };

        let set2 = async {
            Timer::after(Duration::from_millis(10)).await; // ensure that the other task "starts"

            let mut handle = m.write().await;
            assert_eq!(1, *handle.deref());
            *handle.deref_mut() = 2;
        };

        block_on(futures::future::join(set1, set2));
    }
}
