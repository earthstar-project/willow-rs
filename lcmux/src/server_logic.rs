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

use std::{cell::Cell, cmp::min, convert::Infallible, rc::Rc};

use async_cell::unsync::AsyncCell;
use either::Either::*;
use ufotofu::{BufferedProducer, BulkConsumer, BulkProducer, Producer};
use ufotofu_codec::Blame;
use ufotofu_queues::Queue;
use wb_async_utils::spsc;

pub struct SharedState {
    max_queue_capacity: usize,
    /// The number of guarantees we are currently allowed to give to the client.
    /// We accumulate these until we hit the watermark.
    guarantees_to_give: Cell<u64>,
    /// Notify of guarantees to give only once accumulated guarantees have crossed this threshold.
    watermark: u64,
    /// Notify of when the watermark has been reached.
    /// Success is the watermark being reached and guarantees can be given.
    /// Failure is when the client has disrespected its own bound.
    guarantee_related_action_necessary: AsyncCell<Result<(), ()>>,
    droppings_to_announce: AsyncCell<Result<(), ()>>,
    currently_dropping: Cell<bool>,
    their_guarantees: Cell<u64>,
    /// How many more guarantees the client will use at most.
    guarantees_bound: Cell<Option<u64>>,
    no_more_data_will_ever_arrive: AsyncCell<()>,
}

/// Creates the three endpoint for managing the server-side state of a single logical channel.
///
/// This function uses the given queue (of the given `max_queue_capacity`) to buffer incoming bytes. It never issues guarantees exceeding that capacity. It only starts actually granting guarantees once the amount of free space exceeds the watermark.
///
/// - The queue's max capacity must match the given `max_queue_capacity`.
/// - `max_queue_capacity` must be greater than zero.
/// - `watermark` must be less than or equal to `max_queue_capacity`.
pub fn new_logical_channel_server_logic_state<Q>(
    max_queue_capacity: usize,
    watermark: usize,
    queue: Q,
) -> ServerHandle<Q> {
    let state = Rc::new(SharedState {
        max_queue_capacity,
        guarantees_to_give: Cell::new(max_queue_capacity as u64),
        watermark: watermark as u64,
        guarantee_related_action_necessary: AsyncCell::new_with(Ok(())),
        droppings_to_announce: AsyncCell::new(),
        currently_dropping: Cell::new(false),
        their_guarantees: Cell::new(0),
        guarantees_bound: Cell::new(None),
        no_more_data_will_ever_arrive: AsyncCell::new(),
    });

    let (buffer_in, buffer_out) = spsc::new_spsc(queue);

    ServerHandle {
        input: Input {
            buffer_in,
            state: state.clone(),
        },
        guarantees_to_give: GuaranteesToGive {
            state: state.clone(),
        },
        droppings_to_announce: DroppingsToAnnounce {
            state: state.clone(),
        },
        received_data: ReceivedData { buffer_out, state },
    }
}

pub struct GuaranteeOverflow;

impl<F, PE> From<GuaranteeOverflow> for ReceiveDataError<F, PE> {
    fn from(_value: GuaranteeOverflow) -> Self {
        ReceiveDataError::GuaranteeOverflow
    }
}

impl SharedState {
    fn increase_guarantees_to_give(&self, amount: u64) -> Result<(), GuaranteeOverflow> {
        let current_gtg = self.guarantees_to_give.get();
        let next_gtg = current_gtg.checked_add(amount).ok_or(GuaranteeOverflow)?;
        self.guarantees_to_give.set(next_gtg);

        // Have we reached the watermark?
        // If so, set that!
        if next_gtg >= self.watermark {
            self.guarantee_related_action_necessary.set(Ok(()));
        }

        Ok(())
    }

    fn decrease_their_guarantees<Q: Queue>(
        &self,
        amount: u64,
        buffer_in: &mut spsc::Input<Q, (), ()>,
    ) -> Result<(), GuaranteeOverflow> {
        let current_their_guarantees = self.their_guarantees.get();
        let next_their_guarantees = current_their_guarantees
            .checked_sub(amount)
            .ok_or(GuaranteeOverflow)?;
        self.their_guarantees.set(next_their_guarantees);

        // Check if we have a known bound and decrease that bound as well
        if let Some(bound) = self.guarantees_bound.get() {
            let next_bound = bound.saturating_sub(amount);

            self.guarantees_bound.set(Some(next_bound));

            // Reached a bound of zero? Notify the queue that no more data will arrive.
            if next_bound == 0 {
                buffer_in.close_sync(());
            }
        }

        Ok(())
    }
}

pub enum ReceiveDataError<ProducerFinal, ProducerError> {
    UnexpectedEndOfInput(ProducerFinal),
    ProducerError(ProducerError),
    DisrespectedLimitSendingMessage,
    GuaranteeOverflow,
}

pub struct Input<Q> {
    state: Rc<SharedState>,
    buffer_in: spsc::Input<Q, (), ()>,
}

impl<Q: Queue<Item = u8>> Input<Q> {
    /// Reads the next `length` bytes from the given producer, modifying internal state and erroring as appropriate.
    pub async fn receive_data<P: BulkProducer<Item = u8>>(
        &mut self,
        length: u64,
        producer: &mut P,
    ) -> Result<(), ReceiveDataError<P::Final, P::Error>> {
        // Check if the amount is larger than state.guarantees_bound
        if let Some(bound) = self.state.guarantees_bound.get() {
            if length > bound {
                self.buffer_in.cause_error(());
                self.state.guarantee_related_action_necessary.set(Err(()));
                self.state.droppings_to_announce.set(Err(()));

                return Err(ReceiveDataError::DisrespectedLimitSendingMessage);
            }
        }

        // Check if we're currently dropping.
        if self.state.currently_dropping.get() {
            //  if so, do nothing.
            return Ok(());
        } else {
            // otherwise...
            // Check whether we have enough capacity.
            if (self.state.max_queue_capacity - self.buffer_in.len()) as u64 >= length {
                //  and if so, feed the next length-many produced bytes into state.buffer_in.
                bulk_pipe_exact(producer, &mut self.buffer_in, length).await?;

                //    (and decrease state.their_guarantees by length)
                //    (and decrease state.guarantees_bound by length...)
                self.state
                    .decrease_their_guarantees(length, &mut self.buffer_in)?;
            } else {
                //  otherwise, announce a dropping, and change our state to indicate that we are now dropping things on this channel.
                self.state.droppings_to_announce.set(Ok(()));
                self.state.currently_dropping.set(true);
            }
        }

        Ok(())
    }

    /// Updates the server state after receiving an `Absolve` message of a given amount.
    ///
    /// An error indicates an overflow which never occurs with a spec conformant peer.
    pub fn receive_absolve(&mut self, amount: u64) -> Result<(), GuaranteeOverflow> {
        // The other peer reduced their available guarantees by some amount.
        // We now decrease state.their_guarantees by amount,
        self.state
            .decrease_their_guarantees(amount, &mut self.buffer_in)?;
        // and increase state.guarantees_to_give by amount.
        self.state.increase_guarantees_to_give(amount)?;

        Ok(())
    }

    pub fn receive_limit_sending(&mut self, bound: u64) {
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

    pub fn receive_apology(&mut self) {
        // Stop being mad at the other peer, i.e. set state.currently_dropping to false.
        self.state.currently_dropping.set(false)
    }
}

/// A producer of how many guarantees should be granted to the client.
/// NOTE: Once it produces some guarantees, it really definitely assumes those guarantees **will** be granted via a transmitted `IssueGuarantee` message.
pub struct GuaranteesToGive {
    state: Rc<SharedState>,
}

impl Producer for GuaranteesToGive {
    type Item = u64;

    type Final = Infallible;

    type Error = Blame;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        let () = self
            .state
            .guarantee_related_action_necessary
            .get()
            .await
            .or(Err(Blame::TheirFault))?;
        Ok(Left(self.state.guarantees_to_give.get()))
    }
}

/// A producer of events each of which indicating that it is time to transmit an `AnnounceDropping`(s) messages.
pub struct DroppingsToAnnounce {
    state: Rc<SharedState>,
}

impl Producer for DroppingsToAnnounce {
    type Item = ();

    type Final = Infallible;

    type Error = Blame;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        Ok(Left(
            self.state
                .droppings_to_announce
                .get()
                .await
                .or(Err(Blame::TheirFault))?,
        ))
    }
}

/// A `BulkProducer` of all data being sent to the logical channel.
pub struct ReceivedData<Q> {
    state: Rc<SharedState>,
    buffer_out: spsc::Output<Q, (), ()>,
}
// This is a wrapper around SharedState.buffer_out that also calls `increase_guarantees_to_give` for every produced byte.
impl<Q> Producer for ReceivedData<Q>
where
    Q: Queue<Item = u8>,
{
    type Item = u8;

    type Final = ();

    type Error = ();

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        match self.buffer_out.produce().await? {
            Left(byte) => {
                self.state.increase_guarantees_to_give(1).or(Err(()))?;
                Ok(Left(byte))
            }
            Right(fin) => Ok(Right(fin)),
        }
    }
}

impl<Q> BufferedProducer for ReceivedData<Q>
where
    Q: Queue<Item = u8>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        self.buffer_out.slurp().await
    }
}

impl<Q> BulkProducer for ReceivedData<Q>
where
    Q: Queue<Item = u8>,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<either::Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        self.buffer_out.expose_items().await
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.buffer_out.consider_produced(amount).await?;
        self.state
            .increase_guarantees_to_give(amount as u64)
            .or(Err(()))
    }
}

/// Like `bulk_pipe`, but piping exactly `n` many items (stopping after `n` many, and returning an error if the final value is emitted before).
/// Except hardcoded for the needs of this file... Rework this at some point!
async fn bulk_pipe_exact<P, C>(
    producer: &mut P,
    consumer: &mut C,
    n: u64,
) -> Result<(), ReceiveDataError<P::Final, P::Error>>
where
    P: BulkProducer,
    P::Item: Copy,
    C: BulkConsumer<Item = P::Item, Final = (), Error = ()>,
{
    let mut count = 0;

    loop {
        match producer.expose_items().await {
            Ok(Left(items)) => {
                let length: u64 = min(min(n - count, items.len() as u64), usize::MAX as u64);
                let items = &items[..length as usize];
                let amount = match consumer.bulk_consume(items).await {
                    Ok(amount) => amount,
                    Err(_consumer_error) => unreachable!(),
                };
                match producer.consider_produced(amount).await {
                    Ok(()) => {
                        count += amount as u64;
                        // Continue with next loop iteration.
                    }
                    Err(producer_error) => {
                        return Err(ReceiveDataError::ProducerError(producer_error))
                    }
                };
            }
            Ok(Right(final_value)) => {
                return Err(ReceiveDataError::UnexpectedEndOfInput(final_value));
            }
            Err(producer_error) => {
                return Err(ReceiveDataError::ProducerError(producer_error));
            }
        }
    }
}

pub struct ServerHandle<Q> {
    pub input: Input<Q>,
    pub guarantees_to_give: GuaranteesToGive,
    pub droppings_to_announce: DroppingsToAnnounce,
    pub received_data: ReceivedData<Q>,
}
