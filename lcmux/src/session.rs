use ufotofu::{BulkProducer, Producer};

use std::marker::PhantomData;

use wb_async_utils::Mutex;

use crate::client::{
    new_logical_channel_client_state, AbsolutionsToGrant, Input, LogicalChannelClientEndpoint,
};

/// Given a producer and consumer of bytes that represent an ordered, bidirectional, byte-oriented, reliable communication channel with a peer,
/// provide multiplexing and demultiplexing based on LCMUX.
///
/// - `'state` is the lifetime of the opaque [`LcmuxState`] value that the seperate components of the returned [`Lcmux`] struct use to communicate and coordinate.
/// - `'transport` is the lifetime of the producer and consumer that represent the communication channel with the peer; these must outlive the state.
/// - `P` and `C` are the specific producer and consumer types of the communication channel.
/// - `CMessage` is the type of all messages that can be received via [`SendControl`](https://willowprotocol.org/specs/resource-control/index.html#SendControl) messages.
pub fn new_lcmux<'state, 'transport: 'state, const NUM_CHANNELS: usize, P, C, CMessage>(
    state: &'state LcmuxState<'transport, NUM_CHANNELS, P, C, CMessage>,
) -> Lcmux<'state, NUM_CHANNELS, P, C, CMessage> {
    let control_message_producer = ControlMessageProducer { state };

    Lcmux {
        control_message_producer,
        how_to_send_messages_to_a_logical_channel: todo!(),
        tmp: (PhantomData, PhantomData),
    }
}

/// The shared state between the several components of an LCMUX session. This is fully opaque, simply initialise it (via [`LxmucState::new`]) and supply a reference to [`new_lcmux`].
pub struct LcmuxState<'transport, const NUM_CHANNELS: usize, P, C, CMessage> {
    producer: &'transport Mutex<P>,
    consumer: &'transport Mutex<C>,
    client_stuff: [(
        Input,
        LogicalChannelClientEndpoint<'transport, C>,
        AbsolutionsToGrant,
    ); NUM_CHANNELS],
    phantom: PhantomData<CMessage>, // TODO remove this once the `CMessage` type parameter is actually used
}

impl<'transport, const NUM_CHANNELS: usize, P, C, CMessage>
    LcmuxState<'transport, NUM_CHANNELS, P, C, CMessage>
{
    pub fn new(producer: &'transport Mutex<P>, consumer: &'transport Mutex<C>) -> Self {
        LcmuxState {
            producer,
            consumer,
            client_stuff: core::array::from_fn(|channel_id| {
                new_logical_channel_client_state(consumer, channel_id as u64)
            }),
            phantom: PhantomData,
        }
    }
}

/// All components for interacting with an LCMUX session.
pub struct Lcmux<'state, const NUM_CHANNELS: usize, P, C, CMessage> {
    pub control_message_producer: ControlMessageProducer<'state, NUM_CHANNELS, P, C, CMessage>,
    // control_message_consumer TODO
    pub how_to_send_messages_to_a_logical_channel:
        [LogicalChannelClientEndpoint<'state, C>; NUM_CHANNELS],
    // how_to_Receive_messages_from_a_logical_channel TODO
    tmp: (PhantomData<P>, PhantomData<CMessage>),
}

/// A `Producer` of incoming control messages. Reading data from this producer is what drives processing of *all* incoming messages. If you stop reading from this, no more arriving bytes will be processed, even for non-control messages.
pub struct ControlMessageProducer<'state, const NUM_CHANNELS: usize, P, C, CMessage> {
    state: &'state LcmuxState<'state, NUM_CHANNELS, P, C, CMessage>,
}

// impl<'state, const NUM_CHANNELS: usize, P, C, CMessage> Producer
//     for ControlMessageProducer<'state, NUM_CHANNELS, P, C, CMessage>
// where
//     P: BulkProducer<Item = u8>,
// {
//     type Item = CMessage;

//     type Final = P::Final;

//     type Error = i16;

//     async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
//         // Decode LCMUX frames
//         todo!() // interact with self.state and stuff
//     }
// }
