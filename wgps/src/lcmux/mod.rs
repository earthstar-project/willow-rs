use std::marker::PhantomData;

use wb_async_utils::Mutex;

/// Given a producer and consumer of bytes that represent an ordered, bidirectional, byte-oriented, reliable communication channel with a peer,
/// provide multiplexing and demultiplexing based on LCMUX.
///
/// - `'state` is the lifetime of the opaque [`LcmuxState`] value that the seperate components of the returned [`Lcmux`] struct use to communicate and coordinate.
/// - `'transport` is the lifetime of the producer and consumer that represent the communication channel with the peer; these must outlive the state.
/// - `P` and `C` are the specific producer and consumer types of the communication channel.
/// - `CMessage` is the type of all messages that can be received via [`SendControl`](https://willowprotocol.org/specs/resource-control/index.html#SendControl) messages.
pub fn new_lcmux<'state, 'transport: 'state, P, C, CMessage>(
    state: &'state LcmuxState<'transport, P, C, CMessage>,
) -> Lcmux<'state, P, C, CMessage> {
    let control_message_producer = ControlMessageProducer { state };

    Lcmux {
        control_message_producer,
    }
}

/// The shared state between the several components of an LCMUX session. This is fully opaque, simply initialise it (via [`LxmucState::new`]) and supply a reference to [`new_lcmux`].
pub struct LcmuxState<'transport, P, C, CMessage> {
    producer: &'transport Mutex<P>,
    consumer: &'transport Mutex<C>,
    phantom: PhantomData<CMessage>, // TODO remove this once the `CMessage` type parameter is actually used
}

impl<'transport, P, C, CMessage> LcmuxState<'transport, P, C, CMessage> {
    pub fn new(producer: &'transport Mutex<P>, consumer: &'transport Mutex<C>) -> Self {
        LcmuxState { producer, consumer, phantom: PhantomData }
    }
}

/// All components for interacting with an LCMUX session.
pub struct Lcmux<'state, P, C, CMessage> {
    control_message_producer: ControlMessageProducer<'state, P, C, CMessage>,
    // TODO add more components here...
}

/// A `Producer` of incoming control messages. Reading data from this producer is what drives processing of *all* incoming messages. If you stop reading from this, no more arriving bytes will be processed, even for non-control messages.
pub struct ControlMessageProducer<'state, P, C, CMessage> {
    state: &'state LcmuxState<'state, P, C, CMessage>,
}
