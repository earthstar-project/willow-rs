use core::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use std::collections::VecDeque;
use std::fmt;

/// An optional value that can be set to a `Some` at most once, and which allows to `.await` that time.
///
/// All accesses before setting the value are parked and waked in FIFO order.
pub struct OnceCell<T> {
    state: UnsafeCell<State<T>>, // No references to this escape the lifeitme of a method, and each method creates at most one reference.
}

enum State<T> {
    Set(T),
    Empty(VecDeque<Waker>),
}

impl<T> OnceCell<T> {
    /// Creates a new, empty [`OnceCell`].
    pub fn new() -> Self {
        OnceCell {
            state: UnsafeCell::new(State::Empty(VecDeque::new())),
        }
    }

    /// Creates a new [`OnceCell`] that contains a value.
    pub fn new_with(t: T) -> Self {
        OnceCell {
            state: UnsafeCell::new(State::Set(t)),
        }
    }

    /// Consumes the [`OnceCell`] and returns the wrapped value, if there is any.
    pub fn into_inner(self) -> Option<T> {
        match self.state.into_inner() {
            State::Empty(_) => None,
            State::Set(t) => Some(t),
        }
    }

    /// Obtain a mutable reference to the stored value, or `None` if nothing is stored.
    pub fn try_get_mut(&mut self) -> Option<&mut T> {
        match unsafe { &mut *self.state.get() } {
            State::Empty(_) => None,
            State::Set(ref mut t) => Some(t),
        }
    }

    /// Try to obtain an immutable reference to the stored value, or `None` if nothing is stored.
    pub fn try_get(&self) -> Option<&T> {
        match unsafe { &*self.state.get() } {
            State::Empty(_) => None,
            State::Set(ref t) => Some(t),
        }
    }

    /// Obtain an immutable reference to the stored value, `.await`ing it to be set if necessary.
    pub async fn get(&self) -> &T {
        GetFuture(self).await
    }

    /// Set the value in the cell if it was empty before, return an error and do nothing if is was not empty.
    pub fn set(&self, t: T) -> Result<(), T> {
        match unsafe { &mut *self.state.get() } {
            State::Empty(queue) => {
                for waker in queue.iter() {
                    waker.wake_by_ref();
                }
            }
            State::Set(_) => return Err(t),
        }

        unsafe {
            *self.state.get() = State::Set(t);
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
        match unsafe { &mut *self.0.state.get() } {
            State::Empty(queue) => {
                queue.push_back(cx.waker().clone());
                Poll::Pending
            }
            State::Set(ref t) => Poll::Ready(t),
        }
    }
}
