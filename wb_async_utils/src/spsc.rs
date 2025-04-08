//! This module provides a nonblocking **s**ingle **p**roducer **s**ingle **c**onsumer channel
//! backed by an arbitrary [`Queue`], with a UFOTOFU-based interface.
//!
//! It differs from most other spsc implementations in a few important aspects:
//!
//! - It is not `Send`. This channel does not provide synchronisation across threads, it merely decouples program components executing on the same thread. In a sense, its primary function is working around the limitations on shared mutability imposed by the borrow checker.
//! - It can be backed by arbitrary [`Queue`]s. A common choice would be the [`ufotofu_queues::Fixed`] fixed-capacity queue.
//! - It is unopinionated where the shared state between sender and receiver lives. Most APIs transparently handle the state by placing it on the heap behind a reference counted pointer. Our implementation lets the programmer supply the shared state as an opaque struct. When knowing the lifetime of the sender and receiver, the state can be stack-allocated instead of heap-allocated.
//! - It allows closing with arbitrary `Final` values, and the sender has a method for triggering an error on the receiver.
//! - Dropping the sender or the receiver does not actively inform the other endpoint about anything (but you can query whether the other endpoint has been dropped whenever you feel like it).
//!
//! See [`new_spsc`] for the entrypoint to this module and some examples.

use either::Either::{self, *};
use fairly_unsafe_cell::*;
use std::{
    cell::Cell,
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use ufotofu::{BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer};
use ufotofu_queues::Queue;

use crate::{extend_lifetime, extend_lifetime_mut, Mutex, TakeCell};

/// The state shared between the [`Sender`] and the [`Receiver`]. This is fully opaque, but we expose it to give control over where it is allocated.
#[derive(Debug)]
pub struct State<Q, F, E> {
    // We need a Mutex here because `expose_slots` and `expose_items` can be called concurrently on the two endpoints but would result in a mutable and an immutable borrow coexisting.
    queue: Mutex<Q>,
    // Safety: We never return refs to this from any method, and we never hold a borrow across `.await` points.
    // Hence, no cuncurrent refs can exist.
    last: FairlyUnsafeCell<Option<Result<F, E>>>,
    // We track the number of items in the queue here, so that we can access it without waiting for the Mutex of the queue itself.
    // A bit awkward, but this enables sync access to the current count.
    len: Cell<usize>,
    // Empty while the sender cannot make progress.
    notify_the_sender: TakeCell<()>,
    // Empty while the receiver cannot make progress.
    notify_the_receiver: TakeCell<()>,
    // True iff neither endpoint has been dropped yet.
    nothing_dropped_yet: Cell<bool>,
}

impl<Q: Queue, F, E> State<Q, F, E> {
    /// Creates a new [`State`], using the given queue for the SPSC channel. The queue must have a non-zero maximum capacity.
    pub fn new(queue: Q) -> Self {
        State {
            len: Cell::new(queue.len()),
            queue: Mutex::new(queue),
            last: FairlyUnsafeCell::new(None),
            notify_the_sender: TakeCell::new_with(()),
            notify_the_receiver: TakeCell::new_with(()),
            nothing_dropped_yet: Cell::new(true),
        }
    }

    /// Same as calling [`Consumer::close`] on the Sender, but directly from an immutable reference to the state. This is essentially an escape hatch around the *single producer* part for closing. Ideally, you should not need to use this.
    /// The same invariants as for the regular `close` and `clos_sync` methods on the [`Sender`] apply.
    pub fn close(&self, fin: F) {
        // Store the final value for later access by the Sender.
        let mut last = unsafe { self.last.borrow_mut() };
        debug_assert!(
            last.is_none(),
            "Must not call `close` or `close_sync` multiple times or after calling `cause_error`."
        );
        *last = Some(Ok(fin));

        self.notify_the_receiver.set(());
    }

    /// Same as calling `cause_error` on the Sender, but directly from an immutable reference to the state. This is essentially an escape hatch around the *single producer* part for signaling errors. Ideally, you should not need to use this.
    /// The same invariants as for the regular `cause_error` method on the [`Sender`] apply.
    pub fn cause_error(&self, err: E) {
        let mut last = unsafe { self.last.borrow_mut() };
        debug_assert!(
            last.is_none(),
            "Must not call `cause_error` multiple times or after calling `close` or `close_sync`."
        );
        *last = Some(Err(err));

        self.notify_the_receiver.set(());
    }

    /// Returns whether this has been closed yet or whether an error has been caused yet.
    pub fn has_been_closed_or_errored_yet(&self) -> bool {
        unsafe { self.last.borrow().is_some() }
    }
}

/// Creates a new SPSC channel in the form of a [`Sender`] and a [`Receiver`] endpoint which communicate via the given [`State`].
///
/// An example with a stack-allocated [`State`]:
///
/// ```
/// use wb_async_utils::spsc::*;
/// use ufotofu::*;
///
/// let state: State<_, _, ()> = State::new(ufotofu_queues::Fixed::new(99 /* capacity */));
/// let (mut sender, mut receiver) = new_spsc(&state);
///        
/// pollster::block_on(async {
///     // If the capacity was less than four, you would need to join
///     // a future that sends and a future that receives the items.
///     assert!(sender.consume(300).await.is_ok());
///     assert!(sender.consume(400).await.is_ok());
///     assert!(sender.consume(500).await.is_ok());
///     assert!(sender.close(-17).await.is_ok());
///     assert_eq!(300, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(400, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(500, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(-17, receiver.produce().await.unwrap().unwrap_right());
/// });
/// ```
///
/// An example with a heap-allocated [`State`]:
///
/// ```
/// use wb_async_utils::spsc::*;
/// use ufotofu::*;
///
/// let state: State<_, _, ()> = State::new(ufotofu_queues::Fixed::new(99 /* capacity */));
/// let (mut sender, mut receiver) = new_spsc(std::rc::Rc::new(state));
///        
/// pollster::block_on(async {
///     // If the capacity was less than four, you would need to join
///     // a future that sends and a future that receives the items.
///     assert!(sender.consume(300).await.is_ok());
///     assert!(sender.consume(400).await.is_ok());
///     assert!(sender.consume(500).await.is_ok());
///     assert!(sender.close(-17).await.is_ok());
///     assert_eq!(300, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(400, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(500, receiver.produce().await.unwrap().unwrap_left());
///     assert_eq!(-17, receiver.produce().await.unwrap().unwrap_right());
/// });
/// ```
pub fn new_spsc<R, Q, F, E>(state_ref: R) -> (Sender<R, Q, F, E>, Receiver<R, Q, F, E>)
where
    R: Deref<Target = State<Q, F, E>> + Clone,
{
    (
        Sender {
            state: state_ref.clone(),
            phantom: PhantomData,
        },
        Receiver {
            state: state_ref,
            phantom: PhantomData,
        },
    )
}

/// Allows sending data to the SPSC channel via its [`BulkConsumer`] implementation.
#[derive(Debug)]
pub struct Sender<R: Deref<Target = State<Q, F, E>>, Q, F, E> {
    state: R,
    phantom: PhantomData<(Q, F, E)>,
}

/// Allows receiving data from the SPSC channel via its [`BulkProducer`] implementation.
#[derive(Debug)]
pub struct Receiver<R: Deref<Target = State<Q, F, E>>, Q, F, E> {
    state: R,
    phantom: PhantomData<(Q, F, E)>,
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> Sender<R, Q, F, E> {
    /// Returns the number of items that are currently buffered.
    pub fn len(&self) -> usize {
        self.state.len.get()
    }

    /// Returns whether there are currently no items buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets an error to be emitted on the corresponding `Receiver`.
    /// The error is only emitted there when trying to produce values
    /// via `produce` or `expose_items` (or any method calling one of these),
    /// but never when `slurp`ing or calling `consider_produced`.
    ///
    /// Must not call any of the `Consumer`, `BufferedConsumer`, or `BulkProducer` methods
    /// on this `Receiver` after calling this function, nor `close_sync`.
    /// May call this function at most once per `Receiver`.
    /// Must not call this function after calling `close` or `close_sync`.
    pub fn cause_error(&mut self, err: E) {
        self.state.cause_error(err)
    }

    /// Same as calling [`Consumer::close`], but sync. Must not use this multiple times, after calling `close`, or after calling `cause_error`.
    pub fn close_sync(&mut self, fin: F) {
        self.state.close(fin)
    }

    /// Returns whether the correponding [`Receiver`] has been dropped already.
    pub fn is_receiver_dropped(&self) -> bool {
        self.state.nothing_dropped_yet.get()
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q, F, E> Drop for Sender<R, Q, F, E> {
    fn drop(&mut self) {
        self.state.nothing_dropped_yet.set(false);
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> Consumer for Sender<R, Q, F, E> {
    type Item = Q::Item;

    type Final = F;

    type Error = Infallible;

    /// Writes the item into the buffer queue, waiting for buffer space to
    /// become available (by reading items from the corresponding [`Sender`]) if necessary.
    async fn consume(&mut self, item_: Self::Item) -> Result<(), Self::Error> {
        let mut item = item_;
        loop {
            // Try to buffer the item.
            let did_it_work = {
                // Inside a block to drop the Mutex access before awaiting on the notifier.
                self.state.queue.write().await.deref_mut().enqueue(item)
            };

            match did_it_work {
                // Enqueueing failed.
                Some(item_) => {
                    // Wait for queue space.
                    let () = self.state.notify_the_sender.take().await;
                    // Go into the next iteration of the loop, where enqueeuing is guaranteed to succeed.
                    item = item_;
                }
                // Enqueueing succeeded.
                None => {
                    self.state.len.set(self.state.len.get() + 1);
                    self.state.notify_the_receiver.set(());
                    return Ok(());
                }
            }
        }
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        self.close_sync(fin);
        Ok(())
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> BufferedConsumer for Sender<R, Q, F, E> {
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(()) // Nothing to do here.
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> BulkConsumer for Sender<R, Q, F, E> {
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        loop {
            // Try obtain at least one empty slots.
            match self.state.queue.write().await.deref_mut().expose_slots() {
                // No empty slot available.
                None => {
                    // Wait for queue space.
                    let () = self.state.notify_the_sender.take().await;
                    // Go into the next iteration of the loop, where there will be slots available.
                }
                //Got some empty slots.
                Some(slots) => {
                    // We need to return something which lives for 'a,
                    // but to the compiler's best knowledge, `slots` lives only
                    // for as long as the return value of `self.state.queue.borrow_mut()`,
                    // whose lifetime is limited by the current stack frame.
                    //
                    // We *know* that these slots will have a sufficiently long lifetime,
                    // however, because they sit inside a State which must outlive 'a.
                    // Hence, extending the lifetime to 'a is safe.
                    let slots: &'a mut [Q::Item] = unsafe { extend_lifetime_mut(slots) };
                    return Ok(slots);
                }
            }
        }
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.state
            .queue
            .write()
            .await
            .deref_mut()
            .consider_enqueued(amount);
        self.state.len.set(self.state.len.get() + amount);
        self.state.notify_the_receiver.set(());
        Ok(())
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> Receiver<R, Q, F, E> {
    /// Returns the number of items that are currently buffered.
    pub fn len(&self) -> usize {
        self.state.len.get()
    }

    /// Returns whether there are currently no items buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns whether the correponding [`Sender`] has been dropped already.
    pub fn is_receiver_dropped(&self) -> bool {
        self.state.nothing_dropped_yet.get()
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q, F, E> Drop for Receiver<R, Q, F, E> {
    fn drop(&mut self) {
        self.state.nothing_dropped_yet.set(false);
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> Producer for Receiver<R, Q, F, E> {
    type Item = Q::Item;

    type Final = F;

    type Error = E;

    /// Take an item from the buffer queue, waiting for an item to
    /// become available (by being consumed by the corresponding [`Sender`]) if necessary.
    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        loop {
            // Try to obtain the next item.
            match self.state.queue.write().await.deref_mut().dequeue() {
                // At least one item was in the buffer, return it.
                Some(item) => {
                    self.state.len.set(self.state.len.get() - 1);
                    self.state.notify_the_sender.set(());
                    return Ok(Left(item));
                }
                None => {
                    // Buffer is empty.
                    // But perhaps the final item has been consumed already, or an error was requested?
                    match unsafe { self.state.last.borrow_mut().take() } {
                        Some(Ok(fin)) => {
                            return Ok(Right(fin));
                        }
                        Some(Err(err)) => {
                            return Err(err);
                        }
                        None => {
                            // No last item yet, so we wait until something changes.
                            let () = self.state.notify_the_receiver.take().await;
                            // Go into the next iteration of the loop, where progress will be made.
                        }
                    }
                }
            }
        }
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> BufferedProducer for Receiver<R, Q, F, E> {
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        // Nothing to do really, except that if the buffer is empty and an error was set, then we emit it.
        if self.is_empty() {
            // if self.state.queue.read().await.len() == 0 {
            match unsafe { self.state.last.borrow_mut().take() } {
                None => { /* no-op */ }
                Some(Err(err)) => return Err(err),
                Some(Ok(fin)) => {
                    // Put the fin back in the cell.
                    unsafe { *self.state.last.borrow_mut().deref_mut() = Some(Ok(fin)) }
                }
            }
        }

        Ok(()) // Nothing to do.
    }
}

impl<R: Deref<Target = State<Q, F, E>>, Q: Queue, F, E> BulkProducer for Receiver<R, Q, F, E> {
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        loop {
            // Try to get at least one item.
            match self.state.queue.write().await.deref_mut().expose_items() {
                // No items available
                None => {
                    // But perhaps the final item has been consumed already, or an error was requested?
                    match unsafe { self.state.last.borrow_mut().take() } {
                        Some(Ok(fin)) => {
                            return Ok(Right(fin));
                        }
                        Some(Err(err)) => {
                            return Err(err);
                        }
                        None => {
                            // No last item yet, so we wait until something changes.
                            let () = self.state.notify_the_receiver.take().await;
                            // Go into the next iteration of the loop, where progress will be made.
                        }
                    }
                }
                // Got at least one item
                Some(items) => {
                    // We need to return something which lives for 'a,
                    // but to the compiler's best knowledge, `items` lives only
                    // for as long as the return value of `self.state.queue.borrow_mut()`,
                    // whose lifetime is limited by the current stack frame.
                    //
                    // We *know* that these items will have a sufficiently long lifetime,
                    // however, because they sit inside a State which must outlive 'a.
                    // Hence, extending the lifetime to 'a is safe.
                    let items: &'a [Q::Item] = unsafe { extend_lifetime(items) };
                    return Ok(Left(items));
                }
            }
        }
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.state
            .queue
            .write()
            .await
            .deref_mut()
            .consider_dequeued(amount);
        self.state.len.set(self.state.len.get() - amount);
        self.state.notify_the_sender.set(());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::join;

    use ufotofu_queues::Fixed;

    #[test]
    fn test_spsc_sufficient_capacity() {
        let state: State<_, _, ()> = State::new(Fixed::new(99 /* capacity */));
        let (mut sender, mut receiver) = new_spsc(&state);

        pollster::block_on(async {
            assert!(sender.consume(300).await.is_ok());
            assert!(sender.consume(400).await.is_ok());
            assert!(sender.consume(500).await.is_ok());
            assert!(sender.close(-17).await.is_ok());
            assert_eq!(300, receiver.produce().await.unwrap().unwrap_left());
            assert_eq!(400, receiver.produce().await.unwrap().unwrap_left());
            assert_eq!(500, receiver.produce().await.unwrap().unwrap_left());
            assert_eq!(-17, receiver.produce().await.unwrap().unwrap_right());
        });
    }

    #[test]
    fn test_spsc_low_capacity() {
        pollster::block_on(async {
            let state: State<_, _, ()> = State::new(Fixed::new(3 /* capacity */));
            let (mut sender, mut receiver) = new_spsc(&state);

            let send_things = async {
                assert!(sender.consume(300).await.is_ok());
                assert!(sender.consume(400).await.is_ok());
                assert!(sender.consume(500).await.is_ok());
                assert!(sender.close(-17).await.is_ok());
            };

            let receive_things = async {
                assert_eq!(300, receiver.produce().await.unwrap().unwrap_left());
                assert_eq!(400, receiver.produce().await.unwrap().unwrap_left());
                assert_eq!(500, receiver.produce().await.unwrap().unwrap_left());
                assert_eq!(-17, receiver.produce().await.unwrap().unwrap_right());
            };

            join!(send_things, receive_things);
        });
    }

    #[test]
    fn test_spsc_immediate_final() {
        pollster::block_on(async {
            let state: State<Fixed<u8>, i16, ()> = State::new(Fixed::new(3 /* capacity */));
            let (mut sender, mut receiver) = new_spsc(&state);

            let send_things = async {
                assert!(sender.close(-17).await.is_ok());
            };

            let receive_things = async {
                assert_eq!(-17, receiver.produce().await.unwrap().unwrap_right());
            };

            join!(send_things, receive_things);
        });
    }

    #[test]
    fn test_spsc_immediate_error() {
        pollster::block_on(async {
            let state: State<Fixed<u8>, i16, i16> = State::new(Fixed::new(3 /* capacity */));
            let (mut sender, mut receiver) = new_spsc(&state);

            let send_things = async {
                sender.cause_error(-17);
            };

            let receive_things = async {
                assert_eq!(-17, receiver.produce().await.unwrap_err());
            };

            join!(send_things, receive_things);
        });
    }

    #[test]
    fn test_spsc_slurp() {
        pollster::block_on(async {
            let state: State<Fixed<u8>, i16, i16> = State::new(Fixed::new(3 /* capacity */));
            let (mut sender, mut receiver) = new_spsc(&state);

            let send_things = async {
                sender.cause_error(-17);
            };

            let receive_things = async {
                assert_eq!(-17, receiver.slurp().await.unwrap_err());
            };

            join!(send_things, receive_things);
        });
    }
}
