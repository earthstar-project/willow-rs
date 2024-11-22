//! This module provides a nonblocking **m**ulti **p**roducer **m**ulti **c**onsumer channel
//! backed by an arbitrary [`Queue`], with a UFOTOFU-based interface.

use async_cell::unsync::AsyncCell;
use either::Either::{self, *};
use std::{cell::RefCell, convert::Infallible, rc::Rc};
use ufotofu::local_nb::{
    BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer,
};
use ufotofu_queues::Queue;

/// Create an MPMC channel, in the form of an [`Input`] that implements [`BulkConsumer`]
/// and an [`Output`] that implements [`BulkProducer`].
pub fn new_mpmc<Q, F>(queue: Q) -> (Input<Q, F>, Output<Q, F>) {
    let state = Rc::new(SpscState {
        queue: RefCell::new(queue),
        notify: AsyncCell::new(),
        fin: RefCell::new(None),
    });

    return (
        Input {
            state: state.clone(),
        },
        Output { state },
    );
}

/// Shared state between the Input and the Output.
#[derive(Debug)]
struct SpscState<Q, F> {
    queue: RefCell<Q>,
    notify: AsyncCell<()>,
    fin: RefCell<Option<F>>,
}

#[derive(Clone, Debug)]
pub struct Input<Q, F> {
    state: Rc<SpscState<Q, F>>,
}

impl<Q: Queue, F> Input<Q, F> {
    /// Return the number of items that are currently buffered.
    pub fn len(&self) -> usize {
        self.state.queue.borrow().len()
    }
}

impl<Q: Queue, F> Consumer for Input<Q, F> {
    type Item = Q::Item;

    type Final = F;

    type Error = Infallible;

    /// Write the item into the buffer queue, waiting for buffer space to
    /// become available (by reading items from the corresponding [`Output`]) if necessary.
    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        loop {
            // Try to buffer the item.
            match self.state.queue.borrow_mut().enqueue(item) {
                // Enqueueing failed.
                Some(_) => {
                    // Wait for queue space.
                    let () = self.state.notify.take().await;
                    // Go into the next iteration of the loop, where enqueeuing is guaranteed to succeed.
                }
                // Enqueueing succeeded.
                None => return Ok(()),
            }
        }
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        // Store the final value for later access by the Output.
        *self.state.fin.borrow_mut() = Some(fin);

        // If the queue is empty, we need to notify the waiting Output (if any) of the final value.
        if self.state.queue.borrow().is_empty() {
            self.state.notify.set(());
        }

        Ok(())
    }
}

impl<Q: Queue, F> BufferedConsumer for Input<Q, F> {
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(()) // Nothing to do here.
    }
}

impl<Q: Queue, F> BulkConsumer for Input<Q, F> {
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        loop {
            // Try obtain at least one empty slots.
            match self.state.queue.borrow_mut().expose_slots() {
                // No empty slot available.
                None => {
                    // Wait for queue space.
                    let () = self.state.notify.take().await;
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
                    // however, because they sit inside an Rc which has a lifetime of 'a.
                    // An Rc keeps its contents alive as long at least as itself.
                    // Thus we know that the slots have a lifetime of at least 'a.
                    // Hence, extending the lifetime to 'a is safe.
                    let slots: &'a mut [Q::Item] = unsafe { extend_lifetime_mut(slots) };
                    return Ok(slots);
                }
            }
        }
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.state.queue.borrow_mut().consider_enqueued(amount);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Output<Q, F> {
    state: Rc<SpscState<Q, F>>,
}

impl<Q: Queue, F> Output<Q, F> {
    /// Return the number of items that are currently buffered.
    pub fn len(&self) -> usize {
        self.state.queue.borrow().len()
    }
}

impl<Q: Queue, F> Producer for Output<Q, F> {
    type Item = Q::Item;

    type Final = F;

    type Error = Infallible;

    /// Take an item from the buffer queue, waiting for an item to
    /// become available (by being consumed by the corresponding [`Input`]) if necessary.
    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        loop {
            // Try to obtain the next item.
            match self.state.queue.borrow_mut().dequeue() {
                // At least one item was in the buffer, return it.
                Some(item) => return Ok(Left(item)),
                None => {
                    // Buffer is empty.
                    // But perhaps the final item has been consumed already?
                    match self.state.fin.borrow_mut().take() {
                        Some(fin) => {
                            // Yes, so we can return the final item.
                            return Ok(Right(fin));
                        }
                        None => {
                            // No, so we wait until either a regular item or the final item is consumed.
                            let () = self.state.notify.take().await;
                            // Go into the next iteration of the loop, where progress will be made.
                        }
                    }
                }
            }
        }
    }
}

impl<Q: Queue, F> BufferedProducer for Output<Q, F> {
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(()) // Nothing to do.
    }
}

impl<Q: Queue, F> BulkProducer for Output<Q, F> {
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        loop {
            // Try to get at least one item.
            match self.state.queue.borrow_mut().expose_items() {
                // No items available
                None => {
                    // But perhaps the final item has been consumed already?
                    match self.state.fin.borrow_mut().take() {
                        Some(fin) => {
                            // Yes, so we can return the final item.
                            return Ok(Right(fin));
                        }
                        None => {
                            // No, so we wait until either a regular item or the final item is consumed.
                            let () = self.state.notify.take().await;
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
                    // however, because they sit inside an Rc which has a lifetime of 'a.
                    // An Rc keeps its contents alive as long at least as itself.
                    // Thus we know that the items have a lifetime of at least 'a.
                    // Hence, extending the lifetime to 'a is safe.
                    let items: &'a [Q::Item] = unsafe { extend_lifetime(items) };
                    return Ok(Left(items));
                }
            }
        }
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.state.queue.borrow_mut().consider_dequeued(amount);
        Ok(())
    }
}

// This is safe if and only if the object pointed at by `reference` lives for at least `'longer`.
// See https://doc.rust-lang.org/nightly/std/intrinsics/fn.transmute.html for more detail.
unsafe fn extend_lifetime<'shorter, 'longer, T: ?Sized>(reference: &'shorter T) -> &'longer T {
    std::mem::transmute::<&'shorter T, &'longer T>(reference)
}

// This is safe if and only if the object pointed at by `reference` lives for at least `'longer`.
// See https://doc.rust-lang.org/nightly/std/intrinsics/fn.transmute.html for more detail.
unsafe fn extend_lifetime_mut<'shorter, 'longer, T: ?Sized>(
    reference: &'shorter mut T,
) -> &'longer mut T {
    std::mem::transmute::<&'shorter mut T, &'longer mut T>(reference)
}