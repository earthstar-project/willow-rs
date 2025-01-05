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

use crate::extend_lifetime;

/// An optional value that can be set to a `Some` at most once, and which allows to `.await` that time.
///
/// All accesses before setting the value are parked and waked in FIFO order.
pub struct OnceCell<T> {
    state: FairlyUnsafeCell<State<T>>, // No references to this escape the lifeitme of a method, and each method creates at most one reference.
}

enum State<T> {
    Set(T),
    Empty(VecDeque<Waker>),
}

impl<T> Default for OnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> OnceCell<T> {
    /// Creates a new, empty [`OnceCell`].
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let c = OnceCell::<()>::new();
    /// assert_eq!(None, c.into_inner());
    /// ```
    pub fn new() -> Self {
        OnceCell {
            state: FairlyUnsafeCell::new(State::Empty(VecDeque::new())),
        }
    }

    /// Creates a new [`OnceCell`] that contains a value.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let c = OnceCell::new_with(17);
    /// assert_eq!(Some(17), c.into_inner());
    /// ```
    pub fn new_with(t: T) -> Self {
        OnceCell {
            state: FairlyUnsafeCell::new(State::Set(t)),
        }
    }

    /// Consumes the [`OnceCell`] and returns the wrapped value, if there is any.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let c = OnceCell::new_with(17);
    /// assert_eq!(Some(17), c.into_inner());
    /// ```
    pub fn into_inner(self) -> Option<T> {
        match self.state.into_inner() {
            State::Empty(_) => None,
            State::Set(t) => Some(t),
        }
    }

    /// Returns whether the [`OnceCell`] is currently empty.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let c1 = OnceCell::<()>::new();
    /// assert!(c1.is_empty());
    ///
    /// let c2 = OnceCell::new_with(17);
    /// assert!(!c2.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        match unsafe { self.state.borrow().deref() } {
            State::Empty(_) => true,
            State::Set(_) => false,
        }
    }

    /// Obtain a mutable reference to the stored value, or `None` if nothing is stored.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let mut c1 = OnceCell::<()>::new();
    /// assert_eq!(None, c1.try_get_mut());
    ///
    /// let mut c2 = OnceCell::new_with(17);
    /// assert_eq!(Some(&mut 17), c2.try_get_mut());
    /// ```
    pub fn try_get_mut(&mut self) -> Option<&mut T> {
        match self.state.get_mut() {
            State::Empty(_) => None,
            State::Set(ref mut t) => Some(t),
        }
    }

    /// Try to obtain an immutable reference to the stored value, or `None` if nothing is stored.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let c1 = OnceCell::<()>::new();
    /// assert_eq!(None, c1.try_get());
    ///
    /// let c2 = OnceCell::new_with(17);
    /// assert_eq!(Some(&17), c2.try_get());
    /// ```
    pub fn try_get(&self) -> Option<&T> {
        match unsafe { self.state.borrow().deref() } {
            State::Empty(_) => None,
            State::Set(ref t) => Some(unsafe {
                // As long as `&self` is alive, it is impossible to get mutable access to the stored value,
                // since `try_get_mut` needs a mutable reference to the cell.
                extend_lifetime(t)
            }),
        }
    }

    /// Obtain an immutable reference to the stored value, `.await`ing it to be set if necessary.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let cell = OnceCell::new();
    ///
    /// pollster::block_on(async {
    ///     futures::join!(async {
    ///         assert_eq!(&5, cell.get().await);
    ///     }, async {
    ///         assert_eq!(Ok(()), cell.set(5));
    ///     });
    /// });
    /// ```
    pub async fn get(&self) -> &T {
        GetFuture(self).await
    }

    /// Set the value in the cell if it was empty before, return an error and do nothing if is was not empty.
    ///
    /// ```
    /// use wb_async_utils::OnceCell;
    ///
    /// let cell = OnceCell::new();
    ///
    /// pollster::block_on(async {
    ///     futures::join!(async {
    ///         assert_eq!(&5, cell.get().await);
    ///         assert_eq!(Err(17), cell.set(17));
    ///     }, async {
    ///         assert_eq!(Ok(()), cell.set(5));
    ///     });
    /// });
    /// ```
    pub fn set(&self, t: T) -> Result<(), T> {
        match unsafe { self.state.borrow_mut().deref_mut() } {
            State::Empty(queue) => {
                for waker in queue.iter() {
                    waker.wake_by_ref();
                }
            }
            State::Set(_) => return Err(t),
        }

        unsafe {
            *self.state.borrow_mut().deref_mut() = State::Set(t);
        }

        Ok(())
    }
}

impl<T: fmt::Debug> fmt::Debug for OnceCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("OnceCell");
        match self.try_get() {
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

struct GetFuture<'cell, T>(&'cell OnceCell<T>);

impl<'cell, T> Future for GetFuture<'cell, T> {
    type Output = &'cell T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match unsafe { &mut self.0.state.borrow_mut().deref_mut() } {
            State::Empty(queue) => {
                queue.push_back(cx.waker().clone());
                Poll::Pending
            }
            State::Set(ref t) => Poll::Ready(unsafe {
                // As long as the reference to the cell is alive, it is impossible to get mutable access to the stored value,
                // since `try_get_mut` needs a mutable reference to the cell.
                extend_lifetime(t)
            }),
        }
    }
}
