use std::{marker::PhantomData, ops::DerefMut};

use either::Either::{self, *};

use ufotofu::{BulkProducer, Producer};

use ufotofu_codec::{Blame, Decodable, DecodeError, RelativeDecodable};
use ufotofu_queues::Fixed;
use wb_async_utils::Mutex;

use crate::{
    client::{
        self, new_logical_channel_client_state, AbsolutionsToGrant, ClientHandle,
        LogicalChannelClientEndpoint,
    },
    frames::*,
    server_logic::{self, new_logical_channel_server_logic_state},
};

/// Given a producer and consumer of bytes that represent an ordered, bidirectional, byte-oriented, reliable communication channel with a peer,
/// provide multiplexing and demultiplexing based on LCMUX.
///
/// - `P` and `C` are the specific producer and consumer types of the communication channel.
/// - `CMessage` is the type of all messages that can be received via [`SendControl`](https://willowprotocol.org/specs/resource-control/index.html#SendControl) frames.
/// - `Q` is the type of queues that should be used to buffer the message bytes received on a logical channel.
///
/// `NUM_CHANNELS` denotes how many of the channels are actively used, their channel ids range from zero to `NUM_CHANNELS - 1`. Receiving any frame for a channel of id `NUM_CHANNEL` or greater is reported as a fatal error.
///
/// - `max_queue_capacity` must be greater than zero.
/// - `watermark` must be less than or equal to `max_queue_capacity`.
pub fn new_lcmux<'transport, const NUM_CHANNELS: usize, P, C, CMessage>(
    producer: &'transport Mutex<P>,
    consumer: &'transport Mutex<C>,
    max_queue_capacity: usize,
    watermark: usize,
) -> Lcmux<'transport, NUM_CHANNELS, P, C, CMessage> {
    let client_handles: [_; NUM_CHANNELS] = core::array::from_fn(|channel_id| {
        new_logical_channel_client_state(consumer, channel_id as u64)
    });
    let server_handles: [_; NUM_CHANNELS] = core::array::from_fn(|_| {
        new_logical_channel_server_logic_state(
            max_queue_capacity,
            watermark,
            Fixed::<u8>::new(max_queue_capacity),
        )
    });

    let mut client_inputs: [client::Input; NUM_CHANNELS] = todo!(); // TODO adapt https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e76eb546c6e973d693e7c089e9a1f305 (Thanks, Frando!)
    let mut server_inputs: [server_logic::Input<Fixed<u8>>; NUM_CHANNELS] = todo!();

    let control_message_producer = ControlMessageProducer {
        producer,
        client_inputs,
        server_inputs,
        phantom: PhantomData,
    };

    Lcmux {
        control_message_producer,
        how_to_send_messages_to_a_logical_channel: todo!(),
        tmp: PhantomData,
    }
}

/// All components for interacting with an LCMUX session.
pub struct Lcmux<'transport, const NUM_CHANNELS: usize, P, C, CMessage> {
    pub control_message_producer: ControlMessageProducer<'transport, NUM_CHANNELS, P, CMessage>,
    // control_message_consumer TODO
    pub how_to_send_messages_to_a_logical_channel:
        [LogicalChannelClientEndpoint<'transport, C>; NUM_CHANNELS],
    // how_to_Receive_messages_from_a_logical_channel TODO
    tmp: PhantomData<C>,
}

/// A `Producer` of incoming control messages. Reading data from this producer is what drives processing of *all* incoming messages. If you stop reading from this, no more arriving bytes will be processed, even for non-control messages.
pub struct ControlMessageProducer<'transport, const NUM_CHANNELS: usize, P, CMessage> {
    producer: &'transport Mutex<P>,
    client_inputs: [client::Input; NUM_CHANNELS],
    server_inputs: [server_logic::Input<Fixed<u8>>; NUM_CHANNELS],
    phantom: PhantomData<CMessage>,
}

impl<'transport, const NUM_CHANNELS: usize, P, CMessage> Producer
    for ControlMessageProducer<'transport, NUM_CHANNELS, P, CMessage>
where
    P: BulkProducer<Item = u8>,
    CMessage: RelativeDecodable<SendControlNibble, Blame>,
{
    type Item = CMessage;

    type Final = P::Final;

    type Error = DecodeError<P::Final, P::Error, Blame>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        // Decode LCMUX frames until finding a control message.
        // For all non-control messages, update the appropriate client states or server states.
        loop {
            let mut p = self.producer.write().await;

            // The `continue` statements below don't actually skip anything, they just emphasise that the next step is continuing to loop.

            match IncomingFrameHeader::decode(p.deref_mut()).await? {
                // Forward incoming IssueGuarantee frames to the indicated client input.
                IncomingFrameHeader::IssueGuarantee(IssueGuarantee { channel, amount }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.client_inputs[channel as usize]
                            .receive_guarantees(amount)
                            .or(Err(DecodeError::Other(Blame::TheirFault)))?;
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // Forward incoming Absolve frames to the indicated server input.
                IncomingFrameHeader::Absolve(Absolve { channel, amount }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_inputs[channel as usize]
                            .receive_absolve(amount)
                            .or(Err(DecodeError::Other(Blame::TheirFault)))?;
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // Forward incoming Plead frames to the indicated client input.
                IncomingFrameHeader::Plead(Plead { channel, target }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.client_inputs[channel as usize].receive_plead(target);
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // Forward incoming LimitSending frames to the indicated server input.
                IncomingFrameHeader::LimitSending(LimitSending { channel, bound }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_inputs[channel as usize].receive_limit_sending(bound);
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // Forward incoming LimitReceiving frames to the indicated client input.
                IncomingFrameHeader::LimitReceiving(LimitReceiving { channel, bound }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.client_inputs[channel as usize].receive_limit_receiving(bound);
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // AnnounceDropping frames should never arrive, because we do not send optimistically.
                IncomingFrameHeader::AnnounceDropping(AnnounceDropping { channel: _ }) => {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }

                // Forward incoming Apologise frames to the indicated server input.
                IncomingFrameHeader::Apologise(Apologise { channel }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_inputs[channel as usize].receive_apology();
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // When receiving a SendToChannel frame header, forward the indicated amount of bytes into the corresponding buffer.
                IncomingFrameHeader::SendToChannelHeader(SendToChannelHeader {
                    channel,
                    length,
                }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_inputs[channel as usize]
                            .receive_data(length, p.deref_mut())
                            .await
                            .or(Err(DecodeError::Other(Blame::TheirFault)))?;
                        continue;
                    } else {
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }

                // When receiving a SendControl frame header, we can actually produce an item! Yay!
                IncomingFrameHeader::SendControlHeader(SendControlHeader { encoding_nibble }) => {
                    match p.deref_mut().expose_items().await? {
                        Left(_) => {} // no-op
                        Right(fin) => return Ok(Right(fin)),
                    }

                    return Ok(Left(
                        CMessage::relative_decode(p.deref_mut(), &encoding_nibble).await?,
                    ));
                }
            }
        }
    }
}
