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
    guarantees_to_give: AsyncCell<u64>,
    droppings_to_announce: AsyncCell<()>,
    currently_dropping: Cell<bool>,
    their_guarantees: Cell<u64>,
    /// How many more guarantees the client will use at most.
    guarantees_bound: Cell<Option<u64>>,
}

struct Input<Q: Queue> {
    state: Rc<SharedState<Q>>,
}

impl<Q: Queue> Input<Q> {
    async fn receive_data<P: BulkProducer<Item = u8>>(&mut self, length: usize, producer: P) {
        // Check if the amount is larger than state.guarantees_bound
        //    if it is! Enter an error state, all producers (Data, GuaranteesToGive, DroppingsToAnnounce should now emit Final or probably an Error).
        // Check if we're currently dropping.
        //  if so, do nothing.
        // otherwise...
        // Check whether we have enough capacity.
        //  and if so, feed the next length-many produced bytes into state.buffer_in.
        //    (and decrease state.their_guarantees by length)
        //    (and decrease state.guarantees_bound by length...)
        //  otherwise, announce a dropping, and change our state to indicate that we are now dropping things on this channel.
    }

    fn receive_absolve(&mut self, amount: u64) {
        // The other peer reduced their available guarantees by some amount.
        // We now decrease state.their_guarantees by amount,
        // and increase state.guarantees_to_give by amount.
    }

    fn receive_limit_sending(&mut self, bound: u64) {
        // high level: this is about emitting the Final value of the Data producer, GuaranteesToGive producer, and DroppingsToAnnounce producer.
        // set state.guarantees_bound to bound (if bound is tighter than whatever's there - otherwise ignore)
    }

    fn receive_apologise() {
        // TODO!
    }
}

struct GuaranteesToGive<Q: Queue> {
    state: Rc<SharedState<Q>>,
    /// Notify of guarantees to give only once accumulated guarantees have crossed this threshold.
    watermark: u64,
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
