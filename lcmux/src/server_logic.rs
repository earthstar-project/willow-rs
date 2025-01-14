//! Components for the server side of logical channels, i.e., the side that issues guarantees and enforces them.
//!
//! This implementation provides a bounded queue into which all messages sent over the logical channel are copied. It provides notifications when to send guarantees, so that a peer that respects those guarantees never exceeds the queue capacity. If the capacity is exceeded regardless, the message is dropped, and a notification for sending a `AnnounceDropping` message is emitted. Finally, it emits a `LimitReceiving` message when the queue is closed.
//!
//! As input, the implementation requires the messages sent over the logical channel, as well as notifications about incoming `Absolve` messages (which do not affect its internal buffer size, however), `LimitSending` messages (which allow the implementation to communicate when no more messages will arrive over the logical channel), and `Apologise` messages (which are handled fully transparently to allow receiving further messages over the logical channel).
//!
//! The implementation does not emit `Plead` messages, since it works with a fixed-capacity queue.

use std::{cell::Cell, cmp::min, marker::PhantomData, ops::Deref, usize};

use ufotofu::{BufferedProducer, BulkConsumer, BulkProducer, Producer};

use either::Either::{self, *};

use ufotofu_codec::{Encodable, EncodableKnownSize};
use ufotofu_queues::Queue;
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shelf::{self, new_shelf},
    spsc::{self, new_spsc},
    TakeCell,
};

use crate::{
    frames::{LimitSending, SendToChannelHeader},
    guarantee_bound::GuaranteeBoundCell,
    guarantee_cell::GuaranteeCellWithoutBound,
};

/// The opaque state by which the separate components of a [`ServerLogic`] struct can communicate.
#[derive(Debug)]
pub(crate) struct State<Q> {
    spsc_state: spsc::State<Q, (), ()>,
    // Are we currently dropping all incoming data messages?
    currently_dropping: Cell<bool>,
    // Notifications about announcing the dropping of messages: `Left(())` imdicates we should send an `AnnounceDropping` message. `Right(Ok(()))` indicates we will never have to do so again because the client signalled so. `<Right(Err(()))` indicates we will never have to do so again because of a client error.
    start_dropping: TakeCell<Either<(), Result<(), ()>>>,
    // Tracks the voluntary bounds on guarantees that the client will use up at most. Updated whenever a new message arrives, and raises an error if the bound is violated. When the bound is reached, the byte buffer for this channel is closed.
    bound: GuaranteeBoundCell,
    // Guarantees that we can grant to the client. Updated whenever we interact with the receiver end of the buffer channel.
    guarantees: GuaranteeCellWithoutBound,
    /// Notify of guarantees to give only once accumulated guarantees have crossed this threshold.
    watermark: u64,
    // The unchanging channel id of the channel whose state is being handled here.
    channel_id: u64,
    max_queue_capacity: usize,
}

impl<Q: Queue> State<Q> {
    pub fn new(channel_id: u64, queue: Q, max_queue_capacity: usize, watermark: u64) -> Self {
        Self {
            spsc_state: spsc::State::new(queue),
            currently_dropping: Cell::new(false),
            start_dropping: TakeCell::new(),
            bound: GuaranteeBoundCell::new(),
            guarantees: GuaranteeCellWithoutBound::new(),
            watermark,
            channel_id,
            max_queue_capacity,
        }
    }

    // Everything we need to do when we know that no more data will (or rather, should) ever arrive because a voluntary bound was exactly reached.
    fn bound_of_zero(&self) {
        self.spsc_state.close(());
        self.start_dropping.set(Right(Ok(())));
        self.guarantees.set_final();
    }

    // Report to all components that an error ocurred and the session is now invalid.
    fn report_error(&self) {
        self.spsc_state.cause_error(());
        self.start_dropping.set(Right(Err(())));
        self.guarantees.set_error();
    }
}

/// Everything you need to correctly manage an LCMUX server.
#[derive(Debug)]
pub(crate) struct ServerLogic<R, Q> {
    /// Inform the server logic about incoming messages (control and data) about this logical channel.
    pub receiver: MessageReceiver<R, Q>,
    /// You can await on this to be notified when the server should grant more guarantees to the client.
    pub guarantees_to_give: GuaranteesToGive<R, Q>,
    /// A shelf that tells the server when to send `AnnounceDropping` frames. The final value indicates that that will never become necessary again, either because of a voluntary bound (Ok) or because of invalid message patterns (Err).
    pub start_dropping: StartDropping<R, Q>,
    /// All the data that was sent to this logical channel via SendToChannel frames.
    pub received_data: ReceivedData<R, Q>,
}

impl<R, Q> ServerLogic<R, Q>
where
    R: Deref<Target = State<Q>> + Clone,
{
    /// The effective entrypoint to this module. Given a reference to an opaque state handle, this returns the server session state for a single logical channel.
    pub fn new(state: R) -> Self {
        let (buffer_sender, buffer_receiver) = new_spsc(ProjectSpscState {
            r: state.clone(),
            phantom: PhantomData,
        });

        ServerLogic {
            receiver: MessageReceiver {
                state: state.clone(),
                buffer_sender,
            },
            guarantees_to_give: GuaranteesToGive {
                state: state.clone(),
                phantom: PhantomData,
            },
            start_dropping: StartDropping {
                state: state.clone(),
                phantom: PhantomData,
            },
            received_data: ReceivedData {
                state: state,
                buffer_receiver,
            },
        }
    }
}

#[derive(Debug)]
pub enum ReceiveSendToChannelError<ProducerFinal, ProducerError> {
    UnexpectedEndOfInput(ProducerFinal),
    ProducerError(ProducerError),
    DisrespectedLimitSendingMessage,
}

#[derive(Debug)]
pub struct MessageReceiver<R, Q> {
    state: R,
    buffer_sender: spsc::Sender<ProjectSpscState<R, Q>, Q, (), ()>,
}

impl<R, Q> MessageReceiver<R, Q>
where
    R: Deref<Target = State<Q>>,
    Q: Queue<Item = u8>,
{
    /// Reads data from the producer into the buffer. To be called whenever receiving a `SendToChannel` header.
    pub async fn receive_send_to_channel<P: BulkProducer<Item = u8>>(
        &mut self,
        length: u64,
        p: &mut P,
    ) -> Result<(), ReceiveSendToChannelError<P::Final, P::Error>> {
        let state = self.state.deref();

        // If we are currently dropping, do so, silently.
        if state.currently_dropping.get() {
            Ok(())
        } else {
            // Not dropping (yet). Does the message fit into our buffer?
            if length > (state.max_queue_capacity - self.buffer_sender.len()) as u64 {
                // Message is too large, drop it and all further messages until the next apology.
                state.currently_dropping.set(true);
                state.start_dropping.set(Left(()));
                return Ok(());
            }

            // The message would fit into our buffer. Check if it violates a voluntary bound.
            match state.bound.use_up(length) {
                Err(()) => {
                    // Bound was violated. Drop the message, report the error.
                    state.report_error();
                    return Err(ReceiveSendToChannelError::DisrespectedLimitSendingMessage);
                }
                Ok(Some(0)) => {
                    // Bound was exactly reached. We know no more data will arive.
                    state.bound_of_zero();
                    // Continue processing the data.
                }
                Ok(_) => {
                    // Continue processing the data.
                }
            }

            // Actually buffer the data.
            let mut remaining = length;
            while remaining > 0 {
                match p.expose_items().await {
                    Err(producer_error) => {
                        state.report_error();
                        return Err(ReceiveSendToChannelError::ProducerError(producer_error));
                    }
                    Ok(Right(final_value)) => {
                        state.report_error();
                        return Err(ReceiveSendToChannelError::UnexpectedEndOfInput(final_value));
                    }
                    Ok(Left(items)) => {
                        let num_items_to_read =
                            min(items.len(), min(remaining, usize::MAX as u64) as usize);
                        match self
                            .buffer_sender
                            .bulk_consume(&items[..num_items_to_read])
                            .await
                        {
                            Err(_) => unreachable!(), // Infallible
                            Ok(num_consumed) => match p.consider_produced(num_consumed).await {
                                Err(err) => {
                                    state.report_error();
                                    return Err(ReceiveSendToChannelError::ProducerError(err));
                                }
                                Ok(()) => {
                                    remaining -= num_consumed as u64;
                                }
                            },
                        }
                    }
                }
            }

            Ok(())
        }
    }

    /// To be called when receiving an Absolve frame.
    ///
    /// An error indicates an greater Absolve than what they actually had, an overflow, or a violated bound, none of which occurs with a spec conformant peer.
    pub fn receive_absolve(&mut self, amount: u64) -> Result<(), ()> {
        if self.client_guarantees() < amount {
            self.state.report_error();
            return Err(());
        }

        // Act as if we had read `amount` many bytes via `receive_sent_to_channel` and emitted them on the corresponding receiver.
        if let Err(()) = self.state.guarantees.add_guarantees(amount) {
            self.state.report_error();
            return Err(());
        }
        if let Err(()) = self.state.bound.use_up(amount) {
            self.state.report_error();
            return Err(());
        }

        Ok(())
    }

    /// To be called when receiving a LimitSending frame.
    ///
    /// An error indicates an invalid bound (less tight than a prior bound).
    pub fn receive_limit_sending(&mut self, bound: u64) -> Result<(), ()> {
        if self.state.bound.apply_bound(bound).is_err() {
            self.state.report_error();
            return Err(());
        }

        if bound == 0 {
            self.state.bound_of_zero();
        }

        Ok(())
    }

    /// To be called when receiving an Apologise frame.
    ///
    /// An error indicates an apology sent while the client could not have known that we were dropping (or, in fact, knew for certain that we were not).
    pub fn receive_apologise(&mut self) -> Result<(), ()> {
        if !self.state.currently_dropping.get() {
            self.state.report_error();
            return Err(());
        } else {
            // We are currently dropping, but did we even notify the client about that yet?
            match self.state.start_dropping.peek() {
                None => {
                    // All good, we already notified them.
                    // Actually, we don't know whether us writing the corresponding message was still flushed into the network, but this is the best we can check. And even if they sent their message too soon, no inconsistencies can arise from that.
                    Ok(())
                }
                _ => {
                    // They definitely sent their message before we told them we were dropping. That's non-conformant!
                    self.state.report_error();
                    return Err(());
                }
            }
        }
    }

    // Computes how many guarantees the client has available right now.
    fn client_guarantees(&self) -> u64 {
        let pending_guarantees_to_give = self.state.guarantees.get_current_acc();
        (self.state.max_queue_capacity as u64)
            - ((self.buffer_sender.len() as u64) + pending_guarantees_to_give)
    }
}

#[derive(Debug)]
pub struct GuaranteesToGive<R, Q> {
    state: R,
    phantom: PhantomData<Q>,
}

impl<R, Q> Producer for GuaranteesToGive<R, Q>
where
    R: Deref<Target = State<Q>>,
{
    type Item = u64;

    type Final = ();

    type Error = ();

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        self.state
            .guarantees
            .await_threshold_clear(self.state.watermark)
            .await
    }
}

#[derive(Debug)]
pub struct StartDropping<R, Q> {
    state: R,
    phantom: PhantomData<Q>,
}

impl<R, Q> Producer for StartDropping<R, Q>
where
    R: Deref<Target = State<Q>>,
{
    type Item = ();

    type Final = ();

    type Error = ();

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        match self.state.start_dropping.take().await {
            Left(()) => Ok(Left(())),
            Right(Ok(())) => Ok(Right(())),
            Right(Err(())) => Err(()),
        }
    }
}

/// A `BulkProducer` of all data being sent to the logical channel.
///
/// This is a wrapper around SharedState.buffer_out that also calls `increase_guarantees_to_give` for every produced byte.
#[derive(Debug)]
pub struct ReceivedData<R, Q> {
    state: R,
    buffer_receiver: spsc::Receiver<ProjectSpscState<R, Q>, Q, (), ()>,
}

impl<R, Q> Producer for ReceivedData<R, Q>
where
    R: Deref<Target = State<Q>>,
    Q: Queue<Item = u8>,
{
    type Item = u8;

    type Final = ();

    type Error = ();

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        match self.buffer_receiver.produce().await? {
            Left(byte) => match self.state.guarantees.add_guarantees(1) {
                Err(()) => {
                    self.state.report_error();
                    Err(())
                }
                Ok(()) => Ok(Left(byte)),
            },
            Right(fin) => Ok(Right(fin)),
        }
    }
}

impl<R, Q> BufferedProducer for ReceivedData<R, Q>
where
    R: Deref<Target = State<Q>>,
    Q: Queue<Item = u8>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        self.buffer_receiver.slurp().await
    }
}

impl<R, Q> BulkProducer for ReceivedData<R, Q>
where
    R: Deref<Target = State<Q>>,
    Q: Queue<Item = u8>,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<either::Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        self.buffer_receiver.expose_items().await
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.buffer_receiver.consider_produced(amount).await?;

        match self.state.guarantees.add_guarantees(amount as u64) {
            Err(()) => {
                self.state.report_error();
                Err(())
            }
            Ok(()) => Ok(()),
        }
    }
}

#[derive(Debug)]
struct ProjectSpscState<R, Q> {
    r: R,
    phantom: PhantomData<Q>,
}

impl<R, Q> Clone for ProjectSpscState<R, Q>
where
    R: Clone,
{
    fn clone(&self) -> Self {
        Self {
            r: self.r.clone(),
            phantom: self.phantom.clone(),
        }
    }
}

impl<R, Q> Deref for ProjectSpscState<R, Q>
where
    R: Deref<Target = State<Q>>,
{
    type Target = spsc::State<Q, (), ()>;

    fn deref(&self) -> &Self::Target {
        &self.r.deref().spsc_state
    }
}
