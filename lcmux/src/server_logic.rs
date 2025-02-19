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

use ufotofu_queues::Queue;
use wb_async_utils::{
    spsc::{self, new_spsc},
    TakeCell,
};

use crate::{guarantee_bound::GuaranteeBoundCell, guarantee_cell::GuaranteeCellWithoutBound};

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
    max_queue_capacity: usize,
}

impl<Q: Queue<Item = u8>> State<Q> {
    pub fn new(queue: Q, max_queue_capacity: usize, watermark: u64) -> Self {
        let guarantees = GuaranteeCellWithoutBound::new();
        let result = guarantees.add_guarantees((max_queue_capacity - queue.len()) as u64);
        debug_assert!(result == Ok(()));

        Self {
            spsc_state: spsc::State::new(queue),
            currently_dropping: Cell::new(false),
            start_dropping: TakeCell::new(),
            bound: GuaranteeBoundCell::new(),
            guarantees,
            watermark,
            max_queue_capacity,
        }
    }

    // Everything we need to do when we know that no more data will (or rather, should) ever arrive because a voluntary bound was exactly reached.
    fn bound_of_zero(&self) {
        if !self.spsc_state.has_been_closed_or_errored_yet() {
            self.spsc_state.close(());
        }
        self.start_dropping.set(Right(Ok(())));
        self.guarantees.set_final();
    }

    // Report to all components that an error ocurred and the session is now invalid.
    fn report_error(&self) {
        if !self.spsc_state.has_been_closed_or_errored_yet() {
            self.spsc_state.cause_error(());
        }
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

#[derive(Debug, PartialEq, Eq)]
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
            return self.drop_producer_data(length, p).await;
        } else {
            // Not dropping (yet). Does the message fit into our buffer?
            if length > (state.max_queue_capacity - self.buffer_sender.len()) as u64 {
                // Message is too large, drop it and all further messages until the next apology.
                state.currently_dropping.set(true);
                state.start_dropping.set(Left(()));

                // We need to actively read data from the producer and drop it, and report producer errors if necessary.
                return self.drop_producer_data(length, p).await;
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

                    // Anyway, set our state to not dropping, and report success.
                    self.state.currently_dropping.set(false);
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

    async fn drop_producer_data<P: BulkProducer<Item = u8>>(
        &mut self,
        mut remaining: u64,
        p: &mut P,
    ) -> Result<(), ReceiveSendToChannelError<P::Final, P::Error>> {
        let state = self.state.deref();
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
                    let num_items_to_drop =
                        min(items.len(), min(remaining, usize::MAX as u64) as usize);

                    match p.consider_produced(num_items_to_drop).await {
                        Err(err) => {
                            state.report_error();
                            return Err(ReceiveSendToChannelError::ProducerError(err));
                        }
                        Ok(()) => {
                            remaining -= num_items_to_drop as u64;
                        }
                    }
                }
            }
        }

        return Ok(());
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

#[cfg(test)]
mod tests {
    use super::*;

    use ufotofu::{
        producer::{FromSlice, TestProducer, TestProducerBuilder},
        Producer,
    };

    use ufotofu_queues::Fixed;

    use smol::block_on;

    #[test]
    fn test_happy_case_sending_dropping_apologising() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // Initially, all buffer capacity is granted as guarantees.
            assert_eq!(Ok(4), server_logic.guarantees_to_give.produce_item().await);
            // Initially, we are not dropping.
            assert!(!state.currently_dropping.get());

            // Hence, receiving two bytes of messages works.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(2, &mut data_producer)
                    .await
            );
            assert_eq!(Ok(Left(0)), server_logic.received_data.produce().await);
            assert_eq!(Ok(Left(1)), server_logic.received_data.produce().await);

            // Receiving a third byte also works.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(1, &mut data_producer)
                    .await
            );
            assert_eq!(Ok(Left(2)), server_logic.received_data.produce().await);

            // Receiving two more bytes *still* works, because we already produced all the data on the `server_logic.received_data`.
            // This scenario here corresponds to the client optimistically sending a two byte message despite having only one guarantee left.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(2, &mut data_producer)
                    .await
            );
            // Let us *not* produce those bytes right now, so that they stay in our buffer for a while. At this point, the buffer contains [3, 4].

            // Granting guarantees should consequently grant three guarantees: one for the succesful optimistic byte, and two for the remaining buffer capacity.
            assert_eq!(Ok(3), server_logic.guarantees_to_give.produce_item().await);

            // At this point, the client knows it can safely send two more bytes. Lets have it send three and let it fail.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(3, &mut data_producer)
                    .await
            ); // drops [5, 6, 7]
            assert!(state.currently_dropping.get());

            // Further messages are now also dropped, even if they would fit into the remaining buffer space.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(1, &mut data_producer)
                    .await
            ); // drops [8]
            assert!(state.currently_dropping.get());

            // The server expects an apology.
            assert_eq!(Ok(Left(())), server_logic.start_dropping.produce().await);
            // Hence, receiving an apology does not cause an error.
            assert_eq!(Ok(()), server_logic.receiver.receive_apologise());
            assert!(!state.currently_dropping.get()); // We are not dropping anymore after receiving an apology!

            // Now, the server can accept new data that fits its buffer.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(1, &mut data_producer)
                    .await
            ); // accepts [9]

            // Let's confirm that the correct data was accepted and dropped.
            assert_eq!(Ok(Left(3)), server_logic.received_data.produce().await);
            assert_eq!(Ok(Left(4)), server_logic.received_data.produce().await);
            assert_eq!(Ok(Left(9)), server_logic.received_data.produce().await);

            // We have three guarantees to give now. Let's pretend we got absolved of one.
            assert_eq!(Ok(()), server_logic.receiver.receive_absolve(1));
            // Since we got absolved of one, but did not reduce our buffer capacity, we can now grant one *more* guarantee than before.
            assert_eq!(Ok(4), server_logic.guarantees_to_give.produce_item().await);
        });
    }

    #[test]
    fn test_happy_case_close_indirect() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // Client communicates a bound of zero.
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(0));

            // This closes everything.
            assert_eq!(
                Ok(Right(())),
                server_logic.guarantees_to_give.produce().await
            ); // We don't even give the guarantees that we could have given still.
               // Had the server needed those it shouldn't have closed, then.
            assert_eq!(Ok(Right(())), server_logic.start_dropping.produce().await);
            assert_eq!(Ok(Right(())), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_happy_case_close_exact() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0, 1, 2];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // Client communicates a nonzero bound.
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(3));

            // And then uses up that bound over two messages!
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(2, &mut data_producer)
                    .await
            );
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(1, &mut data_producer)
                    .await
            );

            assert_eq!(Ok(Left(0)), server_logic.received_data.produce().await);
            assert_eq!(Ok(Left(1)), server_logic.received_data.produce().await);
            assert_eq!(Ok(Left(2)), server_logic.received_data.produce().await);

            // This closes everything.
            assert_eq!(
                Ok(Right(())),
                server_logic.guarantees_to_give.produce().await
            ); // We don't even give the guarantees that we could have given still.
               // Had the server needed those it shouldn't have closed, then.
            assert_eq!(Ok(Right(())), server_logic.start_dropping.produce().await);
            assert_eq!(Ok(Right(())), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_ignore_explicit_zero_bound() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0, 1, 2];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // Client communicates a zero bound.
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(0));

            // And then ignores it!
            assert_eq!(
                Err(ReceiveSendToChannelError::DisrespectedLimitSendingMessage),
                server_logic
                    .receiver
                    .receive_send_to_channel(3, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            // Or almost: the received_data report closing (since that might have been queried before the error happened anyway, and it was more simple to implement this way rather than having the error override the closing).
            assert_eq!(Ok(Right(())), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_ignore_bound() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0, 1, 2];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // Client communicates a bound.
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(2));

            // And then ignores it!
            assert_eq!(
                Err(ReceiveSendToChannelError::DisrespectedLimitSendingMessage),
                server_logic
                    .receiver
                    .receive_send_to_channel(3, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_invalid_bound() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // Client communicates a bound.
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(2));

            // And then communicates another bound that is less tight!
            assert_eq!(Err(()), server_logic.receiver.receive_limit_sending(3));

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_end_of_input() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // Trying to read two bytes, but the producer ends too early.
            assert_eq!(
                Err(ReceiveSendToChannelError::UnexpectedEndOfInput(())),
                server_logic
                    .receiver
                    .receive_send_to_channel(2, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere, once we read all the bytes that did make it.
            assert_eq!(Ok(Left(0)), server_logic.received_data.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
        });
    }

    #[test]
    fn test_err_end_of_input_while_dropping() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let msg_data = vec![0, 1, 2, 3, 4, 5, 6];
        let mut data_producer = FromSlice::new(&msg_data[..]);

        block_on(async {
            // The client sends too many bytes, so the server starts dropping.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(5, &mut data_producer)
                    .await
            );
            assert!(state.currently_dropping.get());

            // Trying to read four more bytes, but the producer ends too early.
            assert_eq!(
                Err(ReceiveSendToChannelError::UnexpectedEndOfInput(())),
                server_logic
                    .receiver
                    .receive_send_to_channel(4, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_producer_errs() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let mut data_producer: TestProducer<u8, (), i16> =
            TestProducerBuilder::new(vec![1].into(), Err(-4)).build();

        block_on(async {
            // Trying to read three bytes, but the producer errors after one.
            assert_eq!(
                Err(ReceiveSendToChannelError::ProducerError(-4)),
                server_logic
                    .receiver
                    .receive_send_to_channel(4, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere, after reading the bytes that made it.
            assert_eq!(Ok(Left(1)), server_logic.received_data.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
        });
    }

    #[test]
    fn test_err_producer_errs_while_dropping() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let mut data_producer: TestProducer<u8, (), i16> =
            TestProducerBuilder::new(vec![1, 2, 3, 4, 5, 6].into(), Err(-4)).build();

        block_on(async {
            // The client sends too many bytes, so the server starts dropping.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(5, &mut data_producer)
                    .await
            );
            assert!(state.currently_dropping.get());

            // Trying to read four more bytes, but the producer errors.
            assert_eq!(
                Err(ReceiveSendToChannelError::ProducerError(-4)),
                server_logic
                    .receiver
                    .receive_send_to_channel(4, &mut data_producer)
                    .await
            );

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_absolve_too_great() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // Client absolves more than it can.
            assert_eq!(Ok(Left(4)), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.receiver.receive_absolve(99));

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_absolve_too_early() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // The client absolved us of guarantees it couldn't have known we gave to it.
            assert_eq!(Err(()), server_logic.receiver.receive_absolve(2));

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_absolve_exceeding_bound() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // Client absolves more than its bound allows it to.
            assert_eq!(Ok(Left(4)), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Ok(()), server_logic.receiver.receive_limit_sending(2));
            assert_eq!(Err(()), server_logic.receiver.receive_absolve(3));

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_spurious_apology() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        block_on(async {
            // Client sends an apology for no reason.
            assert_eq!(Err(()), server_logic.receiver.receive_apologise());

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }

    #[test]
    fn test_err_optimistic_apology() {
        let queue_size = 4;
        let state = State::new(Fixed::new(queue_size), queue_size, 2);
        let mut server_logic = ServerLogic::new(&state);

        let mut data_producer: TestProducer<u8, (), i16> =
            TestProducerBuilder::new(vec![1, 2, 3, 4, 5, 6].into(), Err(-4)).build();

        block_on(async {
            // The client sends too many bytes, so the server starts dropping.
            assert_eq!(
                Ok(()),
                server_logic
                    .receiver
                    .receive_send_to_channel(5, &mut data_producer)
                    .await
            );
            assert!(state.currently_dropping.get());

            // We then receive an apology, before we even notified the client that we were mad!
            assert_eq!(Err(()), server_logic.receiver.receive_apologise());

            // This triggers errors everywhere.
            assert_eq!(Err(()), server_logic.guarantees_to_give.produce().await);
            assert_eq!(Err(()), server_logic.start_dropping.produce().await);
            assert_eq!(Err(()), server_logic.received_data.produce().await);
        });
    }
}
