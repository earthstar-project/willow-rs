use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::fmt;
use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

use fairly_unsafe_cell::FairlyUnsafeCell;

/// An async cell akin to an [`Option`], whose value can only be accessed via an async `take` method that non-blocks while the cell is empty.
///
/// All accesses are parked and waked in FIFO order.
pub struct TakeCell<T> {
    value: FairlyUnsafeCell<Option<T>>,
    parked: FairlyUnsafeCell<VecDeque<Waker>>, // push_back to enqueue, pop_front to dequeue
}

impl<T> Default for TakeCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TakeCell<T> {
    /// Creates a new, empty [`TakeCell`].
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c = TakeCell::<()>::new();
    /// assert_eq!(None, c.into_inner());
    /// ```
    pub fn new() -> Self {
        TakeCell {
            value: FairlyUnsafeCell::new(None),
            parked: FairlyUnsafeCell::new(VecDeque::new()),
        }
    }

    /// Creates a new [`TakeCell`] storing the given value.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c = TakeCell::new_with(17);
    /// assert_eq!(Some(17), c.into_inner());
    /// ```
    pub fn new_with(value: T) -> Self {
        TakeCell {
            value: FairlyUnsafeCell::new(Some(value)),
            parked: FairlyUnsafeCell::new(VecDeque::new()),
        }
    }

    /// Consumes the [`TakeCell`] and returns the wrapped value, if any.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c = TakeCell::new_with(17);
    /// assert_eq!(Some(17), c.into_inner());
    /// ```
    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }

    /// Returns whether the [`TakeCell`] is currently empty.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c1 = TakeCell::<()>::new();
    /// assert!(c1.is_empty());
    ///
    /// let c2 = TakeCell::new_with(17);
    /// assert!(!c2.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        let r = unsafe { self.value.borrow() };
        r.deref().is_none()
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
    ///     join!(async {
    ///         assert_eq!(5, cell.take().await);
    ///     }, async {
    ///         cell.set(5);
    ///     });
    /// });
    ///
    ///
    /// let cell2 = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     cell2.set(5);
    ///     cell2.set(6);
    ///     assert_eq!(6, cell2.take().await);
    /// });
    /// ```
    pub fn set(&self, value: T) {
        let _ = self.replace(value);
    }

    /// Sets the value in the cell, and returns the old value (if any). If the cell was empty, wakes the oldest pending async method call that was waiting for a value in the cell.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     join!(async {
    ///         assert_eq!(5, cell.take().await)
    ///     }, async {
    ///         assert_eq!(None, cell.replace(5));
    ///     });
    /// });
    /// ```
    pub fn replace(&self, value: T) -> Option<T> {
        let mut r = unsafe { self.value.borrow_mut() };

        match r.deref_mut().take() {
            None => {
                *r.deref_mut() = Some(value);
                self.wake_next();
                None
            }
            Some(old) => {
                *r.deref_mut() = Some(value);
                Some(old)
            }
        }
    }

    /// Takes the current value out of the cell if there is one, or returns `None` otherwise.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c1 = TakeCell::<()>::new();
    /// assert_eq!(None, c1.try_take());
    ///
    /// let c2 = TakeCell::new_with(17);
    /// assert_eq!(Some(17), c2.try_take());
    /// assert!(c2.is_empty());
    /// ```
    pub fn try_take(&self) -> Option<T> {
        let mut r = unsafe { self.value.borrow_mut() };
        r.deref_mut().take()
    }

    /// Takes the current value out of the cell if there is one, waiting for one to arrive if necessary.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new_with(5);
    ///
    /// pollster::block_on(async {
    ///     assert_eq!(5, cell.take().await);
    ///     assert!(cell.is_empty());
    /// });
    /// ```
    pub async fn take(&self) -> T {
        TakeFuture(self).await
    }

    /// Set the value based on the current value (or abscence thereof).
    ///
    /// If the cell was empty, this wakes the oldest pending async method call that was waiting for a value in the cell.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     join!(async {
    ///         assert_eq!(5, cell.take().await);
    ///     }, async {
    ///         cell.update(|x| match x {
    ///             None => 5,
    ///             Some(_) => 6,
    ///         });
    ///     });
    /// });
    /// ```
    pub fn update(&self, with: impl FnOnce(Option<T>) -> T) {
        self.set(with(self.try_take()))
    }

    /// Fallibly set the value based on the current value (or absence thereof). If `with` returns an `Err`, the cell is emptied.
    ///
    /// If the cell was empty and `with` returned `Ok`, this wakes the oldest pending async method call that was waiting for a value in the cell.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     join!(async {
    ///         assert_eq!(5, cell.take().await);
    ///     }, async {
    ///         assert_eq!(Ok(()), cell.fallible_update(|x| match x {
    ///             None => Ok(5),
    ///             Some(_) => Err(()),
    ///         }));
    ///     });
    /// });
    /// ```
    pub fn fallible_update<E>(
        &self,
        with: impl FnOnce(Option<T>) -> Result<T, E>,
    ) -> Result<(), E> {
        self.set(with(self.try_take())?);
        Ok(())
    }

    /// Returns how many tasks are currently waiting for the cell to be filled.
    ///
    /// ```
    /// use futures::join;
    /// use wb_async_utils::TakeCell;
    ///
    /// let cell = TakeCell::new();
    ///
    /// pollster::block_on(async {
    ///     join!(async {
    ///         assert_eq!(0, cell.count_waiting());
    ///         cell.take().await;
    ///         assert_eq!(0, cell.count_waiting());
    ///     }, async {
    ///         assert_eq!(1, cell.count_waiting());
    ///         cell.set(5);
    ///     });
    /// });
    /// ```
    pub fn count_waiting(&self) -> usize {
        let parked = unsafe { self.parked.borrow() };
        parked.deref().len()
    }

    fn wake_next(&self) {
        let mut parked = unsafe { self.parked.borrow_mut() };

        if let Some(waker) = parked.deref_mut().pop_front() {
            waker.wake()
        }
    }

    fn park(&self, cx: &mut Context<'_>) {
        let mut parked = unsafe { self.parked.borrow_mut() };

        parked.deref_mut().push_back(cx.waker().clone());
    }
}

impl<T: Clone> TakeCell<T> {
    /// Synchronously returns a clone of the current value if there is any, or `None` otherwise. Does not change the cell in any way.
    ///
    /// ```
    /// use wb_async_utils::TakeCell;
    ///
    /// let c1 = TakeCell::<()>::new();
    /// assert_eq!(None, c1.peek());
    ///
    /// let c2 = TakeCell::new_with(17);
    /// assert_eq!(Some(17), c2.peek());
    /// assert!(!c2.is_empty());
    /// ```
    pub fn peek(&self) -> Option<T> {
        let r = unsafe { self.value.borrow_mut() };
        r.deref().clone()
    }
}

impl<T: fmt::Debug> fmt::Debug for TakeCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("TakeCell");
        match unsafe { self.value.borrow().deref() } {
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
