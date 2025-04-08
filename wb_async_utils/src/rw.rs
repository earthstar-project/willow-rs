use core::{
    cell::Cell,
    future::Future,
    hint::unreachable_unchecked,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

use fairly_unsafe_cell::FairlyUnsafeCell;

use crate::{extend_lifetime, extend_lifetime_mut};

/// An awaitable, single-threaded read-write lock. Allows an arbitrary number of concurrent immutable accesses, or a single mutable access.
///
/// Write accesses are parked and waked in FIFO order, but read accesses always have priority.
/// That is, after a write access has completed, all read accesses are granted.
/// Only if there are no pending read accesses is the next (in FIFO order) write access granted.
pub struct RwLock<T> {
    value: FairlyUnsafeCell<T>,
    readers: Cell<Option<usize>>, // `None` while writing, `Some(0)` if there are neither readers nor writers
    parked_reads: FairlyUnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
    parked_writes: FairlyUnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> RwLock<T> {
    /// Creates a new RwLock storing the given value.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    ///
    /// let rw = RwLock::new(5);
    /// assert_eq!(5, rw.into_inner());
    /// ```
    pub fn new(value: T) -> Self {
        RwLock {
            value: FairlyUnsafeCell::new(value),
            readers: Cell::new(Some(0)),
            parked_reads: FairlyUnsafeCell::new(VecDeque::new()),
            parked_writes: FairlyUnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the RwLock and returns the wrapped value.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    ///
    /// let rw = RwLock::new(5);
    /// assert_eq!(5, rw.into_inner());
    /// ```
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Gives read access to the wrapped value, waiting if necessary.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     let handle = rw.read().await;
    ///     assert_eq!(0, *handle.deref());
    /// });
    /// ```
    pub async fn read(&self) -> ReadGuard<T> {
        ReadFuture(self).await
    }

    /// Gives read access if doing so is possible without waiting, returns `None` otherwise.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let rw = RwLock::new(0);
    /// assert_eq!(&0, rw.try_read().unwrap().deref());
    ///
    /// block_on(futures::future::join(async {
    ///     let handle = rw.write().await;
    ///     Timer::after(Duration::from_millis(50)).await;
    /// }, async {
    ///     assert!(rw.try_read().is_none());
    /// }));
    /// ```
    pub fn try_read(&self) -> Option<ReadGuard<T>> {
        let reader_count = self.readers.get()?;
        self.readers.set(Some(reader_count + 1));

        Some(ReadGuard { lock: self })
    }

    /// Gives write access to the wrapped value, waiting if necessary.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    /// use core::ops::{Deref, DerefMut};
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let rw = RwLock::new(0);
    /// block_on(futures::future::join(async {
    ///     let mut handle = rw.write().await;
    ///     // Simulate doing some work.
    ///     Timer::after(Duration::from_millis(50)).await;
    ///     assert_eq!(0, *handle.deref());
    ///     *handle.deref_mut() = 1;
    /// }, async {
    ///     // This future is "faster", but has to wait for the "work" performed by the first one.
    ///     let mut handle = rw.write().await;
    ///     assert_eq!(1, *handle.deref());
    /// }));
    /// ```
    pub async fn write(&self) -> WriteGuard<T> {
        WriteFuture(self).await
    }

    /// Gives write access if doing so is possible without waiting, returns `None` otherwise.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    /// use core::time::Duration;
    /// use smol::{block_on, Timer};
    ///
    /// let rw = RwLock::new(0);
    /// assert_eq!(&0, rw.try_write().unwrap().deref());
    ///
    /// block_on(futures::future::join(async {
    ///     let handle = rw.write().await;
    ///     Timer::after(Duration::from_millis(50)).await;
    /// }, async {
    ///     assert!(rw.try_write().is_none());
    /// }));
    /// ```
    pub fn try_write(&self) -> Option<WriteGuard<T>> {
        match self.readers.get() {
            Some(0) => {
                self.readers.set(None);
                Some(WriteGuard { lock: self })
            }
            _ => None,
        }
    }

    /// Sets the wrapped value.
    /// Needs to `.await` read access internally.
    ///
    /// ```
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     rw.set(1).await;
    ///     assert_eq!(1, *rw.read().await.deref());
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
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(0, rw.replace(1).await);
    ///     assert_eq!(1, *rw.read().await.deref());
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
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     rw.update(|x| x + 1).await;
    ///     assert_eq!(1, *rw.read().await.deref());
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
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(Ok(()), rw.fallible_update(|x| Ok::<u8, i8>(x + 1)).await);
    ///     assert_eq!(1, *rw.read().await.deref());
    ///
    ///     assert_eq!(Err(-17), rw.fallible_update(|_| Err(-17)).await);
    ///     assert_eq!(1, *rw.read().await.deref());
    /// });
    /// ```
    pub async fn fallible_update<E>(&self, with: impl FnOnce(&T) -> Result<T, E>) -> Result<(), E> {
        let mut guard = self.write().await;
        *guard = with(&guard)?;
        Ok(())
    }

    // /// Updates the wrapped value with the given async function.
    // /// Needs to `.await` read access internally.
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
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     rw.mutate(|x| *x += 1).await;
    ///     assert_eq!(1, *rw.read().await.deref());
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
    /// use wb_async_utils::rw::*;
    /// use core::ops::Deref;
    ///
    /// let rw = RwLock::new(0);
    /// pollster::block_on(async {
    ///     assert_eq!(Ok(()), rw.fallible_mutate(|x| {
    ///         *x +=1 ;
    ///         Ok::<(), i8>(())
    ///     }).await);
    ///     assert_eq!(1, *rw.read().await.deref());
    ///
    ///     assert_eq!(Err(-17), rw.fallible_mutate(|_| Err(-17)).await);
    ///     assert_eq!(1, *rw.read().await.deref());
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
        debug_assert_eq!(self.readers.get(), Some(0));

        let there_are_no_pending_reads = unsafe { self.parked_reads.borrow().deref().is_empty() };

        if there_are_no_pending_reads {
            if let Some(next_write) =
                unsafe { self.parked_writes.borrow_mut().deref_mut().pop_front() }
            {
                next_write.wake();
            }
        } else {
            for parked_read in unsafe { self.parked_reads.borrow_mut().deref_mut().drain(..) } {
                parked_read.wake();
            }
        }
    }

    fn park_read(&self, cx: &mut Context<'_>) {
        // Safe because self.parked_reads is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let mut parked_reads = unsafe { self.parked_reads.borrow_mut() };
        parked_reads.deref_mut().push_back(cx.waker().clone());
    }

    fn park_write(&self, cx: &mut Context<'_>) {
        // Safe because self.parked_reads is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let mut parked_writes = unsafe { self.parked_writes.borrow_mut() };
        parked_writes.deref_mut().push_back(cx.waker().clone());
    }
}

impl<T> AsMut<T> for RwLock<T> {
    fn as_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T: fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("RwLock");
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

/// Read-only access to the value stored in a [`RwLock`].
///
/// The wrapped value is accessible via the implementation of `Deref`.
///
/// ```
/// use wb_async_utils::rw::*;
/// use core::ops::Deref;
/// use core::time::Duration;
///
/// let rw = RwLock::new(0);
/// pollster::block_on(async {
///     let handle = rw.read().await;
///     assert_eq!(0, *handle.deref());
/// });
/// ```
#[derive(Debug)]
pub struct ReadGuard<'lock, T> {
    lock: &'lock RwLock<T>,
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        match self.lock.readers.get() {
            None => unsafe { unreachable_unchecked() }, // ReadGuards are only created when there are no writers
            Some(reader_count) => {
                self.lock.readers.set(Some(reader_count - 1)); // never underflows, there is at least one reader (us)
                if reader_count == 1 {
                    self.lock.wake_next();
                }
            }
        }
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let borrowed = unsafe { self.lock.value.borrow() }; // Safe because a ReadGuard can never live at the same time as a WriteGuard or a `&mut Mutex`.
                                                            // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                            // as `self`.
        unsafe { extend_lifetime(borrowed.deref()) }
    }
}

/// Read and write access access to the value stored in a [`RwLock`].
///
/// The wrapped value is accessible via the implementation of `Deref` and `DerefMut`.
///
/// ```
/// use wb_async_utils::rw::*;
/// use core::ops::{Deref, DerefMut};
/// use core::time::Duration;
/// use smol::{block_on, Timer};
///
/// let rw = RwLock::new(0);
/// block_on(futures::future::join(async {
///     let mut handle = rw.write().await;
///     // Simulate doing some work.
///     Timer::after(Duration::from_millis(50)).await;
///     assert_eq!(0, *handle.deref());
///     *handle.deref_mut() = 1;
/// }, async {
///     // This future is "faster", but has to wait for the "work" performed by the first one.
///     let mut handle = rw.read().await;
///     assert_eq!(1, *handle.deref());
/// }));
/// ```
#[derive(Debug)]
pub struct WriteGuard<'lock, T> {
    lock: &'lock RwLock<T>,
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.readers.set(Some(0)); // Guaranteed to have been `None` before.

        self.lock.wake_next();
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let borrowed = unsafe { self.lock.value.borrow() }; // Safe because a WriteGuard can never live at the same time as another guard or a `&mut Mutex`.
                                                            // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                            // as `self`.
        unsafe { extend_lifetime(borrowed.deref()) }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        let mut borrowed = unsafe { self.lock.value.borrow_mut() }; // Safe because a WriteGuard can never live at the same time as another guard or a `&mut Mutex`.
                                                                    // We can only obtain references with a lifetime tied to `borrowed`, but we know the refs to be both alive and exclusive for as long
                                                                    // as `self`.
        unsafe { extend_lifetime_mut(borrowed.deref_mut()) }
    }
}

struct ReadFuture<'lock, T>(&'lock RwLock<T>);

impl<'lock, T> Future for ReadFuture<'lock, T> {
    type Output = ReadGuard<'lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_read() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.0.park_read(cx);
                Poll::Pending
            }
        }
    }
}

struct WriteFuture<'lock, T>(&'lock RwLock<T>);

impl<'lock, T> Future for WriteFuture<'lock, T> {
    type Output = WriteGuard<'lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_write() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.0.park_write(cx);
                Poll::Pending
            }
        }
    }
}
