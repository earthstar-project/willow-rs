use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    hint::unreachable_unchecked,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

/// An awaitable, single-threaded read-write lock. Allows an arbitrary number of concurrent immutable accesses, or a single mutable access.
///
/// Write accesses are parked in waked in FIFO order, but read accesses always have priority.
/// That is, after a write access has completed, all read accesses are granted.
/// Only if there are no pending read accesses is the next (in FIFO order) write access granted.
pub struct RwLock<T> {
    value: UnsafeCell<T>,
    readers: Cell<Option<usize>>, // `None` while writing, `Some(0)` if there are neither readers nor writers
    parked_reads: UnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
    parked_writes: UnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> RwLock<T> {
    /// Creates a new RwLock storing the given value.
    pub fn new(value: T) -> Self {
        RwLock {
            value: UnsafeCell::new(value),
            readers: Cell::new(Some(0)),
            parked_reads: UnsafeCell::new(VecDeque::new()),
            parked_writes: UnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the RwLock and returns the wrapped value.
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Gives read access to the wrapped value, waiting if necessary.
    pub async fn read<'lock>(&'lock self) -> ReadGuard<'lock, T> {
        ReadFuture(self).await
    }

    /// Gives read access if doing so is possible without waiting, returns `None` otherwise.
    pub fn try_read<'lock>(&'lock self) -> Option<ReadGuard<'lock, T>> {
        let reader_count = self.readers.get()?;
        self.readers.set(Some(reader_count + 1));

        Some(ReadGuard { lock: self })
    }

    /// Gives write access to the wrapped value, waiting if necessary.
    pub async fn write<'lock>(&'lock self) -> WriteGuard<'lock, T> {
        WriteFuture(self).await
    }

    /// Gives write access if doing so is possible without waiting, returns `None` otherwise.
    pub fn try_write<'lock>(&'lock self) -> Option<WriteGuard<'lock, T>> {
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
    pub async fn set(&self, to: T) {
        let mut guard = self.write().await;
        *guard = to;
    }

    /// Replaces the wrapped value, and returns the old one.
    /// Needs to `.await` read access internally.
    pub async fn replace(&self, mut to: T) -> T {
        let mut guard = self.write().await;
        core::mem::swap(guard.deref_mut(), &mut to);
        to
    }

    /// Updates the wrapped value with the given function.
    /// Needs to `.await` read access internally.
    pub async fn update(&self, with: impl FnOnce(&T) -> T) {
        let mut guard = self.write().await;
        *guard = with(&guard);
    }

    /// Updates the wrapped value with the successful result of the given function, or propagates the error of the function.
    /// Needs to `.await` read access internally.
    pub async fn fallible_update<E>(&self, with: impl FnOnce(&T) -> Result<T, E>) -> Result<(), E> {
        let mut guard = self.write().await;
        *guard = with(&guard)?;
        Ok(())
    }

    /// Updates the wrapped value with the given async function.
    /// Needs to `.await` read access internally.
    pub async fn update_async<Fut: Future<Output = T>>(&self, with: impl FnOnce(&T) -> Fut) {
        let mut guard = self.write().await;
        *guard = with(&guard).await;
    }

    /// Updates the wrapped value with the successful result of the given async function, or propagates the error of the function.
    /// Needs to `.await` read access internally.
    pub async fn fallible_update_async<E, Fut: Future<Output = Result<T, E>>>(
        &self,
        with: impl FnOnce(&T) -> Fut,
    ) -> Result<(), E> {
        let mut guard = self.write().await;
        *guard = with(&guard).await?;
        Ok(())
    }

    /// Mutates the wrapped value with the given function.
    /// Needs to `.await` read access internally.
    pub async fn mutate(&self, with: impl FnOnce(&mut T) -> ()) {
        let mut guard = self.write().await;
        with(&mut guard)
    }

    /// Mutates the wrapped value with the given fallible function, propagates its error if any.
    /// Needs to `.await` read access internally.
    pub async fn fallible_mutate<E>(
        &self,
        with: impl FnOnce(&mut T) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut guard = self.write().await;
        with(&mut guard)
    }

    /// Mutates the wrapped value with the given async function.
    /// Needs to `.await` read access internally.
    pub async fn mutate_async<Fut: Future<Output = ()>>(&self, with: impl FnOnce(&mut T) -> Fut) {
        let mut guard = self.write().await;
        with(&mut guard).await
    }

    /// Mutates the wrapped value with the given fallible async function, propagates its error if any.
    /// Needs to `.await` read access internally.
    pub async fn fallible_mutate_async<E, Fut: Future<Output = Result<(), E>>>(
        &self,
        with: impl FnOnce(&mut T) -> Fut,
    ) -> Result<(), E> {
        let mut guard = self.write().await;
        with(&mut guard).await
    }

    fn wake_next(&self) {
        debug_assert_eq!(self.readers.get(), Some(0));

        let there_are_no_pending_reads = { (unsafe { &*self.parked_reads.get() }).is_empty() };

        if there_are_no_pending_reads {
            if let Some(next_write) = (unsafe { &mut *self.parked_writes.get() }).pop_front() {
                next_write.wake();
            }
        } else {
            for parked_read in (unsafe { &mut *self.parked_reads.get() }).drain(..) {
                parked_read.wake();
            }
        }
    }

    fn park_read(&self, cx: &mut Context<'_>) {
        // Safe because self.parked_reads is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let parked_reads = unsafe { &mut *self.parked_reads.get() };
        parked_reads.push_back(cx.waker().clone());
    }

    fn park_write(&self, cx: &mut Context<'_>) {
        // Safe because self.parked_reads is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let parked_writes = unsafe { &mut *self.parked_writes.get() };
        parked_writes.push_back(cx.waker().clone());
    }
}

impl<T> AsMut<T> for RwLock<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() } // Safe because a `&mut RwLock` can never live at the same time as a `ReadGuard`, `WriteGuard` or another `&mut RwLock`
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
pub struct ReadGuard<'lock, T> {
    lock: &'lock RwLock<T>,
}

impl<'lock, T> Drop for ReadGuard<'lock, T> {
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

impl<'lock, T> Deref for ReadGuard<'lock, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() } // Safe because a ReadGuard can never live at the same time as a WriteGuard or a `&mut RwLock`
    }
}

/// Read and write access access to the value stored in a [`RwLock`].
///
/// The wrapped value is accessible via the implementation of `Deref` and `DerefMut`.
pub struct WriteGuard<'lock, T> {
    lock: &'lock RwLock<T>,
}

impl<'lock, T> Drop for WriteGuard<'lock, T> {
    fn drop(&mut self) {
        self.lock.readers.set(Some(0)); // Guaranteed to have been `None` before.

        self.lock.wake_next();
    }
}

impl<'lock, T> Deref for WriteGuard<'lock, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() } // Safe because a `&WriteGuard` can never live at the same time as a `&mut WriteGuard` or a `&mut RwLock`
    }
}

impl<'lock, T> DerefMut for WriteGuard<'lock, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() } // Safe because a `&mut WriteGuard` can never live at the same time as another `&mut WriteGuard` or a `&mut RwLock`
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
