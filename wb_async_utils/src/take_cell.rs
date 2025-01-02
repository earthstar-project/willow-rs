use core::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

/// An async cell akin to an [`Option`], whose value can only be accessed via an async `take` method that non-blocks while the cell is empty.
///
/// All accesses are parked and waked in FIFO order.
pub struct TakeCell<T> {
    value: UnsafeCell<Option<T>>,
    parked: UnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> Default for TakeCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TakeCell<T> {
    /// Creates a new, empty [`TakeCell`].
    pub fn new() -> Self {
        TakeCell {
            value: UnsafeCell::new(None),
            parked: UnsafeCell::new(VecDeque::new()),
        }
    }

    /// Creates a new [`TakeCell`] storing the given value.
    pub fn new_with(value: T) -> Self {
        TakeCell {
            value: UnsafeCell::new(Some(value)),
            parked: UnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the [`TakeCell`] and returns the wrapped value, if any.
    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }

    /// Returns whether the [`TakeCell`] is currently empty.
    pub fn is_empty(&self) -> bool {
        unsafe { &*self.value.get() }.is_none()
    }

    /// Returns how many tasks are currently waiting for the cell to be filled.
    pub fn count_waiting(&self) -> usize {
        unsafe { &*self.parked.get() }.len()
    }

    /// Sets the value in the cell. If the cell was empty, wakes up the oldest pending async method call that was waiting for a value in the cell.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     let waitForSetting = async {
    ///         assert_eq!(5, cell.take().await);
    ///     };
    ///     let setToFive = async {
    ///         cell.set(5);
    ///     };
    ///
    ///     join!(waitForSetting, setToFive);
    /// });
    /// ```
    pub fn set(&self, value: T) {
        let _ = self.replace(value);
    }

    /// Sets the value in the cell, and returns the old value (if any). If the cell was empty, wakes the oldest pending async method call that was waiting for a value in the cell.
    pub fn replace(&self, value: T) -> Option<T> {
        match unsafe { &mut *self.value.get() }.take() {
            None => {
                *unsafe { &mut *self.value.get() } = Some(value);
                self.wake_next();
                None
            }
            Some(old) => {
                *unsafe { &mut *self.value.get() } = Some(value);
                Some(old)
            }
        }
    }

    /// Takes the current value out of the cell if there is one, or returns `None` otherwise.
    pub fn try_take(&self) -> Option<T> {
        unsafe { &mut *self.value.get() }.take()
    }

    /// Takes the current value out of the cell if there is one, waiting for one to arrive if necessary.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new_with(5);
    ///
    /// pollster::block_on(async {
    ///     assert_eq!(5, cell.take().await);
    /// });
    /// ```
    pub async fn take(&self) -> T {
        TakeFuture(self).await
    }

    /// Set the value based on the current value (or abscence thereof).
    ///
    /// If the cell was empty, this wakes the oldest pending async method call that was waiting for a value in the cell.
    pub fn update(&self, with: impl FnOnce(Option<T>) -> T) {
        self.set(with(self.try_take()))
    }

    /// Fallibly set the value based on the current value (or abscence thereof). If `with` returns an `Err`, the cell is emptied.
    ///
    /// If the cell was empty and `with` returned `Ok`, this wakes the oldest pending async method call that was waiting for a value in the cell.
    pub fn fallible_update<E>(
        &self,
        with: impl FnOnce(Option<T>) -> Result<T, E>,
    ) -> Result<(), E> {
        self.set(with(self.try_take())?);
        Ok(())
    }

    fn wake_next(&self) {
        if let Some(waker) = (unsafe { &mut *self.parked.get() }).pop_front() {
            waker.wake()
        }
    }

    fn park(&self, cx: &mut Context<'_>) {
        let parked = unsafe { &mut *self.parked.get() };
        parked.push_back(cx.waker().clone());
    }
}

impl<T: fmt::Debug> fmt::Debug for TakeCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("TakeCell");
        match unsafe { &*self.value.get() } {
            Some(t) => {
                d.field(t);
            }
            None => {
                d.field(&format_args!("<empty>"));
            }
        }
        d.finish()
    }
}

struct TakeFuture<'cell, T>(&'cell TakeCell<T>);

impl<T> Future for TakeFuture<'_, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_take() {
            Some(t) => Poll::Ready(t),
            None => {
                self.0.park(cx);
                Poll::Pending
            }
        }
    }
}
