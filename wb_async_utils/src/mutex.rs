use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

/// An awaitable, single-threaded mutex. Only a single reference to the contents of the mutex can exist at any time.
///
/// All accesses are parked and waked in FIFO order.
pub struct Mutex<T> {
    value: UnsafeCell<T>,
    currently_used: Cell<bool>,
    parked: UnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> Mutex<T> {
    /// Creates a new mutex storing the given value.
    pub fn new(value: T) -> Self {
        Mutex {
            value: UnsafeCell::new(value),
            currently_used: Cell::new(false),
            parked: UnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the mutex and returns the wrapped value.
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Gives read access to the wrapped value, waiting if necessary.
    pub async fn read(&self) -> ReadGuard<T> {
        ReadFuture(self).await
    }

    /// Gives read access if doing so is possible without waiting, returns `None` otherwise.
    pub fn try_read(&self) -> Option<ReadGuard<T>> {
        if self.currently_used.get() {
            return None;
        }

        Some(ReadGuard { mutex: self })
    }

    /// Gives write access to the wrapped value, waiting if necessary.
    pub async fn write(&self) -> WriteGuard<T> {
        WriteFuture(self).await
    }

    /// Gives write access if doing so is possible without waiting, returns `None` otherwise.
    pub fn try_write(&self) -> Option<WriteGuard<T>> {
        if self.currently_used.get() {
            return None;
        }

        Some(WriteGuard { mutex: self })
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
    pub async fn mutate(&self, with: impl FnOnce(&mut T)) {
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
        if let Some(waker) = (unsafe { &mut *self.parked.get() }).pop_front() {
            waker.wake()
        }
    }

    fn park(&self, cx: &mut Context<'_>) {
        // Safe because self.parked is only accessed during `wake_next` and `park_waker` and access does not outlive those functions.
        let parked = unsafe { &mut *self.parked.get() };
        parked.push_back(cx.waker().clone());
    }
}

impl<T> AsMut<T> for Mutex<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() } // Safe because a `&mut RwLock` can never live at the same time as a `ReadGuard`, `WriteGuard` or another `&mut RwLock`
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
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
pub struct ReadGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T> Drop for ReadGuard<'mutex, T> {
    fn drop(&mut self) {
        self.mutex.currently_used.set(false);
        self.mutex.wake_next();
    }
}

impl<'mutex, T> Deref for ReadGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.value.get() } // Safe because a ReadGuard can never live at the same time as a WriteGuard or a `&mut RwLock`
    }
}

/// Read and write access access to the value stored in a [`RwLock`].
///
/// The wrapped value is accessible via the implementation of `Deref` and `DerefMut`.
pub struct WriteGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T> Drop for WriteGuard<'mutex, T> {
    fn drop(&mut self) {
        self.mutex.currently_used.set(false);
        self.mutex.wake_next();
    }
}

impl<'mutex, T> Deref for WriteGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.value.get() } // Safe because a `&WriteGuard` can never live at the same time as a `&mut WriteGuard` or a `&mut RwLock`
    }
}

impl<'mutex, T> DerefMut for WriteGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.value.get() } // Safe because a `&mut WriteGuard` can never live at the same time as another `&mut WriteGuard` or a `&mut RwLock`
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
