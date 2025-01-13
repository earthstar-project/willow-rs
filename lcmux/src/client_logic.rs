//! See [`new_client_logic`] fr the entrypoint to this module.
//!
//!
//! ## Scope of this implementation
//!
//!
//! Components for the client side of logical channels, i.e., the side that receives guarantees and respects them. This implementation always respects the granted guarantees, it does not allow for optimistic sending, and has no way of handling the communication that can be caused by overly optimistic sending.
//!
//! For each logical channel to write to, the client tracks the available guarantees at any point in time. It further tracks incoming `Plead` messages. This implementation fully honours all `Plead` messages. Finally, it tracks the bounds communicated by incoming `ControlLimitReceiving` messages.
//!
//! On the outgoing side, the module allows for sending to the logical channel while respecting the available guarantees; the implementation does not support optimistic sending. It further provides a method for sending arbitrary `LimitSending` frames. It provides notifications when `Absolve` messages should be sent in response to incoming `Plead` messages; it does not support the sending of unsolicited absolutions.
//!
//! The implementation does not concern itself with incoming `AnnounceDropping` messages, nor with sending `Apologise` messages, because neither occurs (in communication with a correct peer) since it never sends optimistically.

use std::{num::NonZeroU64, ops::Deref};

use ufotofu::BulkConsumer;

use either::Either::*;

use ufotofu_codec::{Encodable, EncodableKnownSize};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shelf::{self, new_shelf},
};

use crate::{
    frames::{LimitSending, SendToChannelHeader},
    guarantee_cell::{GuaranteeCell, ThresholdOutcome},
};

#[derive(Debug)]
pub(crate) struct State {
    // Absolutions that should be granted to the server. Closed with `()` when we have no guarantees available and know that the server will never grant us any new ones either.
    absolution: shelf::State<NonZeroU64, ()>,
    // Guarantees that the server has granted us, with the option to subscribe to some particular amount becoming available. The `Final` value `()` is emitted when we know that the currently desired amount will never become available (because of a bound on the incoming guarantees).
    guarantees: GuaranteeCell,
    // The unchanging channel id of the channel whose state is being handled here.
    channel_id: u64,
}

impl State {
    pub fn new(channel_id: u64) -> Self {
        Self {
            absolution: shelf::State::new(),
            guarantees: GuaranteeCell::new(),
            channel_id,
        }
    }
}

/// Everything you need to correctly manage an LCMUX client.
pub(crate) struct ClientLogic<R> {
    /// Inform the client logic about incoming messages on this.
    receiver: MessageReceiver<R>,
    /// Receive information about when to grant absolution on this. Whenever a value is read from this shelf, the client logic assumes that the absolution will be granted.
    grant_absolution: shelf::Receiver<ProjectAbsolutionShelf<R>, NonZeroU64, ()>,
    /// Allows sending messages to this channel, also allows sending `LimitSending` frames.
    sender: SendToChannel<R>,
}

impl<R> ClientLogic<R>
where
    R: Deref<Target = State> + Clone,
{
    /// The effective entrypoint to this module. Given a reference to an opaque state handle, this returns the session state for an LCMUX client.
    pub fn new(state: R) -> Self {
        let (shelf_sender, shelf_receiver) = new_shelf(ProjectAbsolutionShelf { r: state.clone() });

        ClientLogic {
            receiver: MessageReceiver {
                state: state.clone(),
                absolution_shelf_sender: shelf_sender,
            },
            grant_absolution: shelf_receiver,
            sender: SendToChannel { state },
        }
    }
}

pub(crate) struct MessageReceiver<R> {
    state: R,
    absolution_shelf_sender: shelf::Sender<ProjectAbsolutionShelf<R>, NonZeroU64, ()>,
}

impl<R> MessageReceiver<R>
where
    R: Deref<Target = State>,
{
    /// Updates the client state with more guarantees. To be called whenever receiving a [IssueGuarantee](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlGuarantee) message.
    ///
    /// Returns an `Err(())` when the available guarantees would exceed the maximum of `2^64 - 1`, or when the guarantees would disrespect a prior bound on guarantees. If either happens, the logical channel should be considered completely unuseable (and usually, the whole connection should be dropped, since we are talking to a buggy or malicious peer).
    pub fn receive_guarantees(&mut self, amount: u64) -> Result<(), ()> {
        let state = self.state.deref();
        state.guarantees.add_guarantees(amount)
    }

    /// Updates the client state after receiving a [`ControlLimitReceiving`](https://willowprotocol.org/specs/sync/index.html#ControlLimitReceiving) message with a given [`bound`](https://willowprotocol.org/specs/sync/index.html#ControlLimitReceivingBound) value.
    ///
    /// Returns an error when given an invalid bound (i.e., bounds that are less tight than a previously communicated bound).
    pub fn receive_limit_receiving(&mut self, bound: u64) -> Result<(), ()> {
        let state = self.state.deref();
        state.guarantees.apply_bound(bound).map_err(|_| ())
    }

    /// Updates the client state after receiving a [`Plead`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOops) message with a given [`target`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOopsTarget) value. Fully accepts the `Plead`, if you want to reject some `Plead`s, you need a different implementation.
    pub fn receive_plead(&mut self, target: NonZeroU64) {
        let state = self.state.deref();
        let reduced_by = state.guarantees.reduce_guarantees_down_to(target.into());

        match NonZeroU64::new(reduced_by) {
            None => {
                // Do nothing
            }
            Some(reduced_by_nz) => {
                self.absolution_shelf_sender
                    .update(|old_absolutions| match old_absolutions {
                        None => Left(reduced_by_nz),
                        Some(Right(())) => Right(()),
                        Some(Left(old)) => Left(old.saturating_add(reduced_by)),
                    });
            }
        }
    }
}

pub(crate) struct SendToChannel<R> {
    state: R,
}

impl<R> SendToChannel<R>
where
    R: Deref<Target = State>,
{
    /// Pause until the desired amount of guarantees is available, then reduce the internal state by that amount. In other words, once the guarantees are available, they *must* be used up by sending messages of that side.
    /// Yields an error if the guarantees are known to never become available (because the server communicated a bound on how many guarantees it will still send at most).
    async fn wait_for_guarantees(&mut self, amount: u64) -> Result<(), ()> {
        let state = self.state.deref();
        match state.guarantees.await_threshold_sub(amount).await {
            ThresholdOutcome::YayReached => Ok(()),
            ThresholdOutcome::NopeBound => Err(()),
        }
    }

    /// Send a message to a logical channel. Waits for the necessary guarantees to become available before requesting exclusive access to the consumer for writing the encoded message (and its header).
    pub async fn send_to_channel<CR, C, M>(
        &mut self,
        consumer: &SharedConsumer<CR, C>,
        message: &M,
    ) -> Result<(), LogicalChannelClientError<C::Error>>
    where
        C: BulkConsumer<Item = u8, Final: Clone, Error: Clone>,
        CR: Deref<Target = shared_consumer::State<C>> + Clone,
        M: EncodableKnownSize,
    {
        let size = message.len_of_encoding() as u64;

        // Wait for the necessary guarantees to become available.
        if let Err(()) = self.wait_for_guarantees(size).await {
            return Err(LogicalChannelClientError::LogicalChannelClosed);
        }

        let header = SendToChannelHeader {
            channel: self.state.deref().channel_id,
            length: size,
        };

        let mut c = consumer.access_consumer().await;
        header.encode(&mut c).await?;
        message.encode(&mut c).await?;

        Ok(())
    }

    /// Send a `LimitSending` frame to the server.
    ///
    /// This method does not check that you send valid limits or that you respect them on the future.
    pub async fn limit_sending<CR, C>(
        &mut self,
        consumer: &SharedConsumer<CR, C>,
        bound: u64,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8, Final: Clone, Error: Clone>,
        CR: Deref<Target = shared_consumer::State<C>> + Clone,
    {
        let frame = LimitSending {
            channel: self.state.deref().channel_id,
            bound,
        };

        let mut c = consumer.access_consumer().await;
        frame.encode(&mut c).await?;

        Ok(())
    }
}

/// The `Error` type for a consumer for logical channel. A final value is either one from the underlying `BulkConsumer` (of type `E`), or a dedicated variant to indicate that the logical channel was closed by the peer.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
pub enum LogicalChannelClientError<E> {
    /// The underlying `BulkConsumer` errored.
    Underlying(E),
    /// The peer closed this logical channel, so it must reject future values.
    LogicalChannelClosed,
}

impl<E> From<E> for LogicalChannelClientError<E> {
    fn from(err: E) -> Self {
        Self::Underlying(err)
    }
}

#[derive(Debug, Clone)]
struct ProjectAbsolutionShelf<R> {
    r: R,
}

impl<R> Deref for ProjectAbsolutionShelf<R>
where
    R: Deref<Target = State>,
{
    type Target = shelf::State<NonZeroU64, ()>;

    fn deref(&self) -> &Self::Target {
        &self.r.deref().absolution
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, time::Duration};

    use super::*;

    use ufotofu::{consumer::IntoVec, ConsumeAtLeastError, Producer};

    use smol::{block_on, Timer};

    #[test]
    fn test_bound() {
        let state = State::new(17);
        let mut client_logic = ClientLogic::new(&state);

        assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(12));
        assert_eq!(Ok(()), client_logic.receiver.receive_limit_receiving(8));
        assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(8));
        assert_eq!(Err(()), client_logic.receiver.receive_guarantees(1));
    }

    #[test]
    fn test_plead() {
        let state = State::new(17);
        let mut client_logic = ClientLogic::new(&state);

        smol::block_on(async {
            assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(12));
            client_logic
                .receiver
                .receive_plead(NonZeroU64::new(5).unwrap());
            assert_eq!(
                Ok(Left(NonZeroU64::new(7).unwrap())),
                client_logic.grant_absolution.produce().await
            );

            client_logic
                .receiver
                .receive_plead(NonZeroU64::new(4).unwrap());
            client_logic
                .receiver
                .receive_plead(NonZeroU64::new(3).unwrap());
            assert_eq!(
                Ok(Left(NonZeroU64::new(2).unwrap())),
                client_logic.grant_absolution.produce().await
            );
        });
    }

    struct TestMessage;

    impl Encodable for TestMessage {
        async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
        where
            C: BulkConsumer<Item = u8>,
        {
            consumer
                .bulk_consume_full_slice(&[1, 2, 3])
                .await
                .map_err(ConsumeAtLeastError::into_reason)
        }
    }

    impl EncodableKnownSize for TestMessage {
        fn len_of_encoding(&self) -> usize {
            3
        }
    }

    #[test]
    fn test_sending() {
        let state = State::new(17);
        let mut client_logic = ClientLogic::new(&state);

        let inner_consumer = IntoVec::new();
        let shared_consumer_state = shared_consumer::State::new(inner_consumer);
        let c = SharedConsumer::new(&shared_consumer_state);

        let stage = Cell::new(0); // for writing assertions about when methods block.

        smol::block_on(async {
            futures::join!(
                async {
                    assert_eq!(
                        Ok(()),
                        client_logic.sender.send_to_channel(&c, &TestMessage).await
                    );
                    stage.set(1);

                    assert_eq!(
                        Ok(()),
                        client_logic.sender.send_to_channel(&c, &TestMessage).await
                    );
                    stage.set(2);

                    assert_eq!(
                        Err(LogicalChannelClientError::LogicalChannelClosed),
                        client_logic.sender.send_to_channel(&c, &TestMessage).await
                    );
                    stage.set(3);
                },
                async {
                    assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(2));
                    Timer::after(Duration::from_millis(20)).await;
                    assert_eq!(0, stage.get());

                    assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(2));
                    Timer::after(Duration::from_millis(20)).await;
                    assert_eq!(1, stage.get());

                    assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(1));
                    Timer::after(Duration::from_millis(20)).await;
                    assert_eq!(1, stage.get());

                    assert_eq!(Ok(()), client_logic.receiver.receive_guarantees(2));
                    Timer::after(Duration::from_millis(20)).await;
                    assert_eq!(2, stage.get());

                    assert_eq!(Ok(()), client_logic.receiver.receive_limit_receiving(1));
                    Timer::after(Duration::from_millis(20)).await;
                    assert_eq!(3, stage.get());
                },
            );
        });
    }
}
