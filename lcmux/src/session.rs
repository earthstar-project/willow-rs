use std::{convert::Infallible, marker::PhantomData, mem::MaybeUninit, ops::DerefMut};

use either::Either::{self, *};
use futures::try_join;

use ufotofu::{BulkConsumer, BulkProducer, Producer};

use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
};
use ufotofu_queues::Fixed;
use wb_async_utils::Mutex;

use crate::{
    client::{self, new_logical_channel_client_state, LogicalChannelClientEndpoint},
    frames::*,
    server_logic::{self, new_logical_channel_server_logic_state},
};

pub use crate::frames::SendControlNibble;

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

    // Conversion from arrays of structs to arrays of individual fields adapted from https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e76eb546c6e973d693e7c089e9a1f305
    // Thank you, Frando!
    let mut client_inputs: [_; NUM_CHANNELS] = std::array::from_fn(|_| MaybeUninit::uninit());
    let mut server_inputs: [_; NUM_CHANNELS] = std::array::from_fn(|_| MaybeUninit::uninit());
    let mut bookkeepings: [_; NUM_CHANNELS] = std::array::from_fn(|_| MaybeUninit::uninit());
    let mut how_to_send_messages_to_a_logical_channel: [_; NUM_CHANNELS] =
        std::array::from_fn(|_| MaybeUninit::uninit());
    let mut receive_data_from_logical_channel: [_; NUM_CHANNELS] =
        std::array::from_fn(|_| MaybeUninit::uninit());

    for (i, (client_handle, server_handle)) in client_handles
        .into_iter()
        .zip(server_handles.into_iter())
        .enumerate()
    {
        client_inputs[i].write(client_handle.input);
        server_inputs[i].write(server_handle.input);
        bookkeepings[i].write(ChannelBookkeeping {
            channel_id: i as u64,
            consumer,
            absolutions_to_grant: client_handle.absolutions,
            guarantees_to_give: server_handle.guarantees_to_give,
            droppings_to_announce: server_handle.droppings_to_announce,
        });
        how_to_send_messages_to_a_logical_channel[i].write(client_handle.logical_consumer);
        receive_data_from_logical_channel[i].write(server_handle.received_data);
    }

    // SAFETY: As N is constant, we know that all elements are initialized.
    let client_inputs = client_inputs.map(|x| unsafe { x.assume_init() });
    let server_inputs = server_inputs.map(|x| unsafe { x.assume_init() });
    let bookkeepings = bookkeepings.map(|x| unsafe { x.assume_init() });
    let how_to_send_messages_to_a_logical_channel =
        how_to_send_messages_to_a_logical_channel.map(|x| unsafe { x.assume_init() });
    let receive_data_from_logical_channel =
        receive_data_from_logical_channel.map(|x| unsafe { x.assume_init() });

    let control_message_producer = ControlMessageProducer {
        producer,
        client_inputs,
        server_inputs,
        phantom: PhantomData,
    };

    let control_message_consumer = ControlMessageSender { consumer };

    Lcmux {
        control_message_producer,
        control_message_sender: control_message_consumer,
        channel_bookkeeping: bookkeepings,
        how_to_send_messages_to_a_logical_channel,
        receive_data_from_logical_channel,
    }
}

/// All components for interacting with an LCMUX session.
pub struct Lcmux<'transport, const NUM_CHANNELS: usize, P, C, CMessage> {
    /// A producer of incoming control messages. You *must* read from this *constantly*, as it also drives internal processing of all other LCMUX frames. When it emits its final item or an error, then the full LCMUX session is done.
    pub control_message_producer: ControlMessageProducer<'transport, NUM_CHANNELS, P, CMessage>,
    /// Takes outgoing control messages and frames, encodes, and transmits them.
    pub control_message_sender: ControlMessageSender<'transport, C>,
    /// For each logical channel an opaque struct whose async `do_the_bookkeeping_dance` method must be called and continuously polled, to carry out all behind-the-scenes bookkeeping transmissions by the peer for the channel.
    pub channel_bookkeeping: [ChannelBookkeeping<'transport, C>; NUM_CHANNELS],
    /// Structs that allow for sending messages to a logical channel, one struct per channel.
    pub how_to_send_messages_to_a_logical_channel:
        [LogicalChannelClientEndpoint<'transport, C>; NUM_CHANNELS],
    // Producers of data received on a logical channel, one producer per channel.
    pub receive_data_from_logical_channel: [server_logic::ReceivedData<Fixed<u8>>; NUM_CHANNELS],
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

/// Sends control messages to send to the other peer.
pub struct ControlMessageSender<'transport, C> {
    consumer: &'transport Mutex<C>,
}

impl<'transport, C> ControlMessageSender<'transport, C>
where
    C: BulkConsumer<Item = u8>,
{
    pub async fn receive<CMessage: RelativeEncodable<SendControlNibble> + GetControlNibble>(
        &mut self,
        item: CMessage,
    ) -> Result<(), C::Error> {
        let mut c = self.consumer.write().await;

        let nibble = item.control_nibble();
        let header = SendControlHeader {
            encoding_nibble: nibble,
        };
        header.encode(c.deref_mut()).await?;

        item.relative_encode(c.deref_mut(), &nibble).await
    }
}

/// A trait for encoding control messages: given a reference to a control message, yields the correspondong [`SendControlNibble`].
pub trait GetControlNibble {
    fn control_nibble(&self) -> SendControlNibble;
}

/// An opaque struct whose async `do_the_bookkeeping_dance` method must be called and continuously polled, to carry out all behind-the-scenes bookkeeping transmissions by the peer.
pub struct ChannelBookkeeping<'transport, C> {
    channel_id: u64,
    consumer: &'transport Mutex<C>,
    absolutions_to_grant: client::AbsolutionsToGrant,
    guarantees_to_give: server_logic::GuaranteesToGive,
    droppings_to_announce: server_logic::DroppingsToAnnounce,
}

impl<'transport, C> ChannelBookkeeping<'transport, C>
where
    C: BulkConsumer<Item = u8>,
{
    /// Performs all sending of metadata to keep the logical channel functioning.
    pub async fn do_the_bookkeeping_dance(
        &mut self,
    ) -> Result<Infallible, Either<C::Error, Blame>> {
        let _ = try_join!(
            Self::grant_all_the_absolutions(
                self.consumer,
                &mut self.absolutions_to_grant,
                self.channel_id
            ),
            Self::give_all_the_guarantees(
                self.consumer,
                &mut self.guarantees_to_give,
                self.channel_id
            ),
            Self::announce_all_the_droppings(
                self.consumer,
                &mut self.droppings_to_announce,
                self.channel_id
            ),
        )?;
        unreachable!()
    }

    /// Continuously reads from the `absolutions_to_grant` and acts on them whenever one is actually produced.
    async fn grant_all_the_absolutions(
        consumer: &'transport Mutex<C>,
        absolutions_to_grant: &mut client::AbsolutionsToGrant,
        channel_id: u64,
    ) -> Result<Infallible, Either<C::Error, Blame>> {
        loop {
            match absolutions_to_grant.produce().await {
                Ok(Left(amount)) => {
                    let frame: Absolve = Absolve {
                        channel: channel_id,
                        amount,
                    };

                    let mut c = consumer.write().await;
                    frame
                        .encode(c.deref_mut())
                        .await
                        .map_err(|con_err| Left(con_err))?;
                }
                _ => unreachable!(), // Final and Error of AbsolutionsToGrant are Infallible. Happy refactoring when that changes!
            }
        }
    }

    /// Continuously reads form the `guarantees_to_give` and gives them.
    async fn give_all_the_guarantees(
        consumer: &'transport Mutex<C>,
        guarantees_to_give: &mut server_logic::GuaranteesToGive,
        channel_id: u64,
    ) -> Result<Infallible, Either<C::Error, Blame>> {
        loop {
            match guarantees_to_give
                .produce()
                .await
                .map_err(|blame| Right(blame))?
            {
                Right(_) => unreachable!(),
                Left(amount) => {
                    let frame = IssueGuarantee {
                        channel: channel_id,
                        amount,
                    };

                    let mut c = consumer.write().await;
                    frame
                        .encode(c.deref_mut())
                        .await
                        .map_err(|con_err| Left(con_err))?;
                }
            }
        }
    }

    /// Continuously reads form the `droppings_to_announce` and does whatever you do with droppings.
    async fn announce_all_the_droppings(
        consumer: &'transport Mutex<C>,
        droppings_to_announce: &mut server_logic::DroppingsToAnnounce,
        channel_id: u64,
    ) -> Result<Infallible, Either<C::Error, Blame>> {
        loop {
            match droppings_to_announce
                .produce()
                .await
                .map_err(|blame| Right(blame))?
            {
                Right(_) => unreachable!(),
                Left(()) => {
                    let frame = AnnounceDropping {
                        channel: channel_id,
                    };

                    let mut c = consumer.write().await;
                    frame
                        .encode(c.deref_mut())
                        .await
                        .map_err(|con_err| Left(con_err))?;
                }
            }
        }
    }
}
