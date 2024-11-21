//! Components for the server side of logical channels, i.e., the side that issues guarantees and enforces them.
//!
//! This implementation provides a bounded queue into which all messages sent over the logical channel are copied. It provides notifications when to send guarantees, so that a peer that respects those guarantees never exceeds the queue capacity. If the capacity is exceeded regardless, the message is dropped, and a notification for sending a `AnnounceDropping` message is emitted. Finally, it emits a `LimitReceiving` message when the queue is closed.
//!
//! As input, the implementation requires the messages pertaining to the logical channel, as well as notifications about incoming `Absolve` messages (which do not affect its internal buffer size, however), `LimitSending` messages (which allow the implementaiton to communicate when no more messages will arrive over the logical channel), and `Apologise` messages (which are handled fully transparently to allow receiving further messages over the logical channel).
//!
//! The implementation does not emit `Plead` messages, since it works with a fixed-capacity queue.
//!
//! Note that this module does not deal with any sort of message encoding or decoding, it merely provides the machinery for tracking and modifying guarantees.

// Implementing correct server behaviour requires working both with incoming data and sending data to an outgoing channel. Both happen concurrently, and both need to be able to modify the state that the server must track. Since rust is not keen on shared mutable state, we follow the common pattern of having a shared state inside some cell(s) inside an Rc, with multiple values having access to the Rc. In particular, we use `AsyncCell`s.

use std::{cell::Cell, rc::Rc};

use crate::util::spsc_channel as spsc;
use async_cell::unsync::AsyncCell;
use ufotofu::local_nb::BulkProducer;
use ufotofu_queues::Queue;

struct SharedState<Q: Queue> {
    buffer_in: spsc::Input<Q>,
    buffer_out: spsc::Output<Q>,
    max_queue_capacity: usize,
    /// The number of guarantees we are currently allowed to give to the client.
    /// We accumulate these until we hit the watermark.
    guarantees_to_give: Cell<u64>,
    /// Notify of guarantees to give only once accumulated guarantees have crossed this threshold.
    watermark: u64,
    /// Notify of when the watermark has been reached.
    crossed_watermark: AsyncCell<()>,
    droppings_to_announce: AsyncCell<()>,
    currently_dropping: Cell<bool>,
    their_guarantees: Cell<u64>,
    /// How many more guarantees the client will use at most.
    guarantees_bound: Cell<Option<u64>>,
    no_more_data_will_ever_arrive: AsyncCell<()>,
}

impl<Q: Queue> SharedState<Q> {
    pub fn increase_guarantees_to_give(&self, amount: u64) -> Result<(), ()> {
        let current_gtg = self.guarantees_to_give.get();
        let next_gtg = current_gtg.checked_add(amount).ok_or(())?;
        self.guarantees_to_give.set(next_gtg);

        // Have we reached the watermark?
        // If so, set that!
        if next_gtg >= self.watermark {
            self.crossed_watermark.set(());
        }

        Ok(())
    }

    pub fn decrease_their_guarantees(&self, amount: u64) -> Result<(), ()> {
        let current_their_guarantees = self.their_guarantees.get();
        let next_their_guarantees = current_their_guarantees.checked_sub(amount).ok_or(())?;
        self.their_guarantees.set(next_their_guarantees);

        // Check if we have a known bound and decrease that bound as well
        if let Some(bound) = self.guarantees_bound.get() {
            let next_bound = bound.saturating_sub(amount);

            self.guarantees_bound.set(Some(next_bound));
        }

        Ok(())
    }
}

struct Input<Q: Queue> {
    state: Rc<SharedState<Q>>,
}

impl<Q: Queue> Input<Q> {
    async fn receive_data<P: BulkProducer<Item = u8>>(
        &mut self,
        length: u64,
        producer: &mut P,
    ) -> Result<(), ()> {
        // Check if the amount is larger than state.guarantees_bound
        if let Some(bound) = self.state.guarantees_bound.get() {
            if length > bound {
                //    if it is! Enter an error state, all producers (Data, GuaranteesToGive, DroppingsToAnnounce should now emit Final or probably an Error).
                return Err(());
            }
        }

        // Check if we're currently dropping.
        if self.state.currently_dropping.get() {
            //  if so, do nothing.
            return Ok(());
        } else {
            // otherwise...
            // Check whether we have enough capacity.
            if (self.state.max_queue_capacity - self.state.buffer_in.len()) as u64 >= length {
                //  and if so, feed the next length-many produced bytes into state.buffer_in.

                bounded_pipe(producer, &mut self.state.buffer_in).await?;

                //    (and decrease state.their_guarantees by length)
                //    (and decrease state.guarantees_bound by length...)
                self.state.decrease_their_guarantees(length);
            } else {
                //  otherwise, announce a dropping, and change our state to indicate that we are now dropping things on this channel.
                self.state.droppings_to_announce.set(());
                self.state.currently_dropping.set(true);
            }
        }

        Ok(())
    }

    /// Updates the server state after receiving an `Absolve` message of a given amount.
    ///
    /// An error indicates an overflow which never occurs with a spec conformant peer.
    fn receive_absolve(&mut self, amount: u64) -> Result<(), ()> {
        // The other peer reduced their available guarantees by some amount.
        // We now decrease state.their_guarantees by amount,
        self.state.decrease_their_guarantees(amount)?;
        // and increase state.guarantees_to_give by amount.
        self.state.increase_guarantees_to_give(amount)?;

        Ok(())
    }

    fn receive_limit_sending(&mut self, bound: u64) {
        // high level: this is about emitting the Final value of the Data producer, GuaranteesToGive producer, and DroppingsToAnnounce producer.
        // set state.guarantees_bound to bound (if bound is tighter than whatever's there - otherwise ignore)
        let new_bound = match self.state.guarantees_bound.get() {
            None => {
                self.state.guarantees_bound.set(Some(bound));
                bound
            }
            Some(old) => {
                let new_bound = core::cmp::min(old, bound);
                self.state.guarantees_bound.set(Some(new_bound));
                new_bound
            }
        };

        if new_bound == 0 {
            // Notify Data that it can emit its final value.
            self.state.no_more_data_will_ever_arrive.set(())
        }
    }

    fn receive_apology(&mut self) {
        // Stop being mad at the other peer, i.e. set state.currently_dropping to false.
        self.state.currently_dropping.set(false)
    }
}

struct GuaranteesToGive<Q: Queue> {
    state: Rc<SharedState<Q>>,
}

// implement producer of u64 for GuaranteesToGive
// and in produce method await state.guarantees_to_give, compare against threshold,
// if guarantees above threshold, empty state.guarantees_to_give and produce value.
// else await on state.guarantees_to_give again.
// do this in a looooooop (not recursively)

struct DroppingsToAnnounce {}

// this is a producer of data taken from SharedState.buffer_out,
// and whenever we produce data we check whether we want to send more guarantees.
// should also keep track of state.guarantees_bound. emit final once that value reaches zero.
struct Data {}

// Do this for real in Ufotofu
async fn bounded_pipe<P, C>(producer: &mut P, consumer: &mut C) -> Result<(), u16> {
    unimplemented!()
}
