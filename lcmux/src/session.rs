// So, about the state of this implementation. It gets things done, but there is room for improvement, and some quirks to be aware of. These are compromises because the spec kept evolving after we had already written the (funded, unlike the subsequent and thus minimal changes) first implementation.
//
// There is no optimistic sending, ever. If your peers do not issue proactive credit on some logical channel, no messages will ever be sent on it. Global messages and other channels will remain unaffected, though. Yay. Still, adding optimistic sending is the most important lacking feature.
//
// All channels are backed by a ringbuffer, and processing data from that rinbuffer always means that as many guarantees will eventually be given on that channel again. The only way of applying backpressure is thus to not read the data in the first place. This does not mesh well with the binding of resource handles, because messages that bind resource handles must be processed immediately. Consequently, this implementation cannot apply proper backpressure for resource handle channels. This, together with the inability to send optimistically, means that for two peers that both use this implementation to work together, they must issue proactive guarantees for the resource handle channel. To prevent a loophole where one peer can be forced to allocate more and more resource handles, you should immeditately send a LimitReceivingFrame for the channel, putting an upper bound on the total number of bytes devoted to binding resource handles, for the whole session. This is not great, but it is what we have right now. To trigger sending this LimitReceivingFrame, use the Session::init method.
//
// Each logical channel is configured further with a boolean urgent flag. Bytes from a non-urgent channel will only be exposed to surrounding code when no bytes are buffered in any urgent channel. By setting channels that bind resource handles as urgent and all other channels as non-urgent, you can ensure that resource handle binding messages are processed "immediately".
//
// Those are the main gotchas, everything else is actually pretty nice. You get BulkProducers and BulkConsumers for each logical channel, and you can use them all independently. So after becoming aware of the quirks, and configuring things accordingly, the surrounding code then gets a pretty nice interface that does not leak any internals of LCMUX.

use std::{
    cell::Cell,
    error::Error,
    fmt::{Debug, Display, Formatter},
    marker::PhantomData,
    mem::MaybeUninit,
    num::NonZeroU64,
    ops::Deref,
};

use either::Either::{self, *};
use futures::{
    future::{try_join3, try_join_all},
    TryFutureExt,
};

use ufotofu::{BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer};

use ufotofu_codec::{
    Decodable, DecodeError, Encodable, EncodableKnownSize, RelativeEncodableKnownSize,
};
use ufotofu_queues::Fixed;
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    TakeCell,
};

use crate::{
    client_logic::{self, ClientLogic},
    frames::*,
    server_logic::{self, ReceiveSendToChannelError, ServerLogic},
};

pub use crate::client_logic::LogicalChannelClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelOptions {
    /// Capacity of the ring buffer that buffers incoming message bytes for this channel.
    buffer_capacity: usize,
    /// How many guarantees must become grantable at least before they are actually granted. If this is greater than the `buffer_capacity`, no guarantees will ever be granted. You probably do not want that.
    watermark: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitOptions {
    /// Is there, from the start, an upper bound on how many bytes this channel wants to receive in total.
    pub receiving_limit: Option<u64>,
}

/// The state for an Lcmux session for channels `0` to `NUM_CHANNELS - 1`.
///
/// This struct is opaque, but we expose it to allow for control over where it is allocated.
#[derive(Debug)]
pub struct State<const NUM_CHANNELS: usize, P, PR, C, CR>
where
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    channel_states: [(client_logic::State, server_logic::State<Fixed<u8>>); NUM_CHANNELS],
    p: SharedProducer<PR, P>,
    c: SharedConsumer<CR, C>,
    number_of_nonempty_urgent_channels: Cell<usize>,
    no_urgency_notifier: TakeCell<()>, // empty while number_of_nonempty_urgent_channels is nonzero
}

impl<const NUM_CHANNELS: usize, P, PR, C, CR> State<NUM_CHANNELS, P, PR, C, CR>
where
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// Create a new opaque state for an LCMUX session.
    ///
    /// The `buffer_capacity` specifies how many bytes each logical channel can buffer at most.
    /// The `watermark` specifies how many guarantees must become grantable at least before they are actually granted.
    pub fn new(
        p: SharedProducer<PR, P>,
        c: SharedConsumer<CR, C>,
        options: [&ChannelOptions; NUM_CHANNELS],
    ) -> Self {
        Self {
            channel_states: core::array::from_fn(|i| {
                (
                    client_logic::State::new(i as u64),
                    server_logic::State::new(
                        Fixed::new(options[i].buffer_capacity),
                        options[i].buffer_capacity,
                        options[i].watermark,
                    ),
                )
            }),
            p,
            c,
            number_of_nonempty_urgent_channels: Cell::new(0),
            no_urgency_notifier: TakeCell::new_with(()),
        }
    }

    fn decrement_urgent_count(&self) {
        let old_count = self.number_of_nonempty_urgent_channels.get();
        self.number_of_nonempty_urgent_channels.set(old_count - 1);

        if old_count == 1 {
            self.no_urgency_notifier.set(());
        }
    }

    async fn wait_for_no_urgency(&self) {
        self.no_urgency_notifier.take().await;
        self.no_urgency_notifier.set(());
    }
}

/// All components for interacting with an LCMUX session. Note that you must call (and poll to completion) the `init` method before working with the session.
///
/// `NUM_CHANNELS` is the number of channels. `R` is the type by which to access the corresponding [`State`].
/// `P` is the type of the producer of the underlying communication channel, and `PR` is the type of references to the shared state of the `SharedProducer<P>`.
/// `C` is the type of the consumer of the underlying communication channel, and `CR` is the type of references to the shared state of the `SharedConsumer<C>`.
#[derive(Debug)]
pub struct Session<
    const NUM_CHANNELS: usize,
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    C: Consumer,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    GlobalMessage,
    GlobalMessageDecodeErrorReason,
> {
    /// A struct with an async function whose Future must be polled to completion to run the LCMUX session. Yields `Ok(())` if every logical channel got closed by both peers, yields an `Err` if *any* channel encounters any error.
    pub bookkeeping: Bookkeeping<NUM_CHANNELS, R, P, PR, C, CR>,
    /// A struct for sending global messages to the other peer.
    pub global_sender: GlobalMessageSender<NUM_CHANNELS, R>,
    /// A struct for receiving global messages from the other peer.
    pub global_receiver: GlobalMessageReceiver<
        NUM_CHANNELS,
        R,
        P,
        PR,
        C,
        CR,
        GlobalMessage,
        GlobalMessageDecodeErrorReason,
    >,
    /// A struct for sending global messages to the other peer.
    /// For each logical channel, a way to send data to that channel via `SendToChannel` frames.
    pub channel_senders: [ChannelSender<NUM_CHANNELS, R, P, PR, C, CR>; NUM_CHANNELS],
    /// For each logical channel, a producer of the bytes that were sent to that channel via `SendToChannel` frames.
    pub channel_receivers: [ChannelReceiver<NUM_CHANNELS, R, P, PR, C, CR>; NUM_CHANNELS],
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR, GlobalMessage, GlobalMessageDecodeErrorReason>
    Session<NUM_CHANNELS, R, P, PR, C, CR, GlobalMessage, GlobalMessageDecodeErrorReason>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>> + Clone,
    P: Producer,
    C: BulkConsumer<Item = u8, Final: Clone, Error: Clone>,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// Creates a new LCMUX session.
    pub fn new(state_ref: R, which_channels_are_urgent: [bool; NUM_CHANNELS]) -> Self {
        let client_logics: [_; NUM_CHANNELS] = core::array::from_fn(|channel_id| {
            ClientLogic::new(ProjectIthClientState {
                r: state_ref.clone(),
                i: channel_id,
                phantom: PhantomData,
            })
        });
        let server_logics: [_; NUM_CHANNELS] = core::array::from_fn(|channel_id| {
            ServerLogic::new(ProjectIthServerState {
                r: state_ref.clone(),
                i: channel_id,
                phantom: PhantomData,
            })
        });

        // Now we do a silly dance to create a bunch of arrays of the components.
        // Thank you, Frando, for this technique: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e76eb546c6e973d693e7c089e9a1f305

        let mut client_receivers: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());
        let mut client_grant_absolutions: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());
        let mut client_senders: [_; NUM_CHANNELS] = std::array::from_fn(|_| MaybeUninit::uninit());
        let mut server_receivers: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());
        let mut server_guarantees_to_gives: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());
        let mut server_start_droppings: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());
        let mut server_received_datas: [_; NUM_CHANNELS] =
            std::array::from_fn(|_| MaybeUninit::uninit());

        for (i, (client_logic, server_logic)) in client_logics
            .into_iter()
            .zip(server_logics.into_iter())
            .enumerate()
        {
            client_receivers[i].write(client_logic.receiver);
            client_grant_absolutions[i].write(client_logic.grant_absolution);
            client_senders[i].write(client_logic.sender);
            server_receivers[i].write(server_logic.receiver);
            server_guarantees_to_gives[i].write(server_logic.guarantees_to_give);
            server_start_droppings[i].write(server_logic.start_dropping);
            server_received_datas[i].write(server_logic.received_data);
        }

        // SAFETY: As N is constant, we know that all elements are initialized.
        let client_receivers = client_receivers.map(|x| unsafe { x.assume_init() });
        let client_grant_absolutions = client_grant_absolutions.map(|x| unsafe { x.assume_init() });
        let client_senders = client_senders.map(|x| unsafe { x.assume_init() });
        let server_receivers = server_receivers.map(|x| unsafe { x.assume_init() });
        let server_guarantees_to_gives =
            server_guarantees_to_gives.map(|x| unsafe { x.assume_init() });
        let server_start_droppings = server_start_droppings.map(|x| unsafe { x.assume_init() });
        let server_received_datas = server_received_datas.map(|x| unsafe { x.assume_init() });

        // The silly dance is over. We have successfully decomposed two arrays into arrays of their components.

        let mut i = 0;

        Self {
            bookkeeping: Bookkeeping {
                state: state_ref.clone(),
                client_grant_absolutions: Some(client_grant_absolutions),
                server_guarantees_to_gives: Some(server_guarantees_to_gives),
                server_start_droppings: Some(server_start_droppings),
            },
            global_sender: GlobalMessageSender {
                state: state_ref.clone(),
            },
            global_receiver: GlobalMessageReceiver {
                state: state_ref.clone(),
                client_receivers,
                server_receivers,
                urgent_channels: which_channels_are_urgent,
                phantom: PhantomData,
            },
            channel_senders: client_senders.map(|send_to_channel| ChannelSender(send_to_channel)),
            channel_receivers: server_received_datas.map(|receive_data| {
                let ret = ChannelReceiver {
                    received_data: receive_data,
                    session_state: state_ref.clone(),
                    is_urgent: which_channels_are_urgent[i],
                };
                i += 1;
                ret
            }),
        }
    }

    /// Performs one-off transmissions at the start of the session. Call this method (and poll it to completion) before doing any other work with the session.
    pub async fn init(&mut self, options: [InitOptions; NUM_CHANNELS]) -> Result<(), C::Error> {
        for i in 0..NUM_CHANNELS {
            if let Some(limit) = options[0].receiving_limit {
                let frame = LimitReceiving {
                    channel: i as u64,
                    bound: limit,
                };

                let mut c = self.bookkeeping.state.deref().c.access_consumer().await;
                frame.encode(&mut c).await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ChannelSender<const NUM_CHANNELS: usize, R, P, PR, C, CR>(
    client_logic::SendToChannel<ProjectIthClientState<NUM_CHANNELS, R, P, PR, C, CR>>,
);

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> ChannelSender<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: BulkConsumer<Item = u8, Final: Clone, Error: Clone>,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// Send a message to a logical channel. Waits for the necessary guarantees to become available before requesting exclusive access to the consumer for writing the encoded message (and its header).
    pub async fn send_to_channel<M>(
        &mut self,
        message: &M,
    ) -> Result<(), LogicalChannelClientError<C::Error>>
    where
        M: EncodableKnownSize,
    {
        let consumer = self.0.state.r.c.clone();
        self.0.send_to_channel(&consumer, message).await
    }

    /// Send a message to a logical channel, using a relative encoding. Waits for the necessary guarantees to become available before requesting exclusive access to the consumer for writing the encoded message (and its header).
    pub async fn send_to_channel_relative<M, RelativeTo>(
        &mut self,
        message: &M,
        relative_to: &RelativeTo,
    ) -> Result<(), LogicalChannelClientError<C::Error>>
    where
        M: RelativeEncodableKnownSize<RelativeTo>,
    {
        let consumer = self.0.state.r.c.clone();
        self.0
            .send_to_channel_relative(&consumer, message, relative_to)
            .await
    }

    /// Send a `LimitSending` frame to the server.
    ///
    /// This method does not check that you send valid limits or that you respect them on the future.
    pub async fn limit_sending(&mut self, bound: u64) -> Result<(), C::Error> {
        let consumer = self.0.state.r.c.clone();
        self.0.limit_sending(&consumer, bound).await
    }

    /// Same as `self.limit_sending(0)`.
    pub async fn close(&mut self) -> Result<(), C::Error> {
        self.limit_sending(0).await
    }
}

#[derive(Debug)]
pub struct ChannelReceiver<
    const NUM_CHANNELS: usize,
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    C: Consumer,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
> {
    received_data:
        server_logic::ReceivedData<ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>, Fixed<u8>>,
    session_state: R,
    is_urgent: bool,
}

// (server_logic::ReceivedData<ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>, Fixed<u8>>, bool);

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Producer
    for ChannelReceiver<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    type Item = u8;

    type Final = ();

    type Error = ();

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        loop {
            if self.is_urgent {
                match self.received_data.produce().await {
                    Ok(Left(yay)) => {
                        if self.received_data.is_buffer_empty() {
                            self.session_state.deref().decrement_urgent_count();
                            return Ok(Left(yay));
                        }
                    }
                    other => return other,
                }
            } else if self.session_state.number_of_nonempty_urgent_channels.get() == 0 {
                return self.received_data.produce().await;
            } else {
                self.session_state.deref().wait_for_no_urgency().await;
            }
        }
    }
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> BufferedProducer
    for ChannelReceiver<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        self.received_data.slurp().await
    }
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> BulkProducer
    for ChannelReceiver<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<either::Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        loop {
            if self.is_urgent || self.session_state.number_of_nonempty_urgent_channels.get() == 0 {
                return self.received_data.expose_items().await;
            } else {
                self.session_state.deref().wait_for_no_urgency().await;
            }
        }
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.received_data.consider_produced(amount).await?;

        if self.is_urgent && self.received_data.is_buffer_empty() {
            self.session_state.deref().decrement_urgent_count();
        }

        Ok(())
    }
}

/// Call and poll to completion the `keep_the_books` method on this struct to run an LCMUX session.
#[derive(Debug)]
pub struct Bookkeeping<const NUM_CHANNELS: usize, R, P, PR, C, CR> {
    state: R,
    client_grant_absolutions: Option<
        [client_logic::GrantAbsolution<ProjectIthClientState<NUM_CHANNELS, R, P, PR, C, CR>>;
            NUM_CHANNELS],
    >,
    server_guarantees_to_gives: Option<
        [server_logic::GuaranteesToGive<
            ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>,
            Fixed<u8>,
        >; NUM_CHANNELS],
    >,
    server_start_droppings: Option<
        [server_logic::StartDropping<
            ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>,
            Fixed<u8>,
        >; NUM_CHANNELS],
    >,
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Bookkeeping<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer<Item = u8, Final = (), Error: Clone> + BulkConsumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// You must call this function once and poll it to completion to run the session. Otherwise, GrantAbsolution, GiveGuarantees, and StartDropping frames will not be sent.
    ///
    /// Finally yields `Ok` if all logical channels were closed by both peers, or yields an error immediately when any part of the LCMUX session causes an error. More precisely, yields `None` if the peer misbehaved, or an underlying transport error if that occured.
    ///
    /// Calling this function multiple times gives unspecified behaviour.
    pub async fn keep_the_books(&mut self) -> Result<(), Option<C::Error>> {
        let grant_absolutions = self
            .client_grant_absolutions
            .take()
            .expect("Must call keep_the_books at most once.");

        let guarantees_to_gives = self
            .server_guarantees_to_gives
            .take()
            .expect("Must call keep_the_books at most once.");

        let start_droppings = self
            .server_start_droppings
            .take()
            .expect("Must call keep_the_books at most once.");

        let grant_all_absolutions = try_join_all(grant_absolutions.into_iter().enumerate().map(
            |(i, mut grant_absolutions)| {
                let shared_c = self.state.c.clone();

                async move {
                    loop {
                        match grant_absolutions.produce().await {
                            Err(_) => unreachable!(), // Infallible
                            Ok(Left(amount)) => {
                                let frame = Absolve {
                                    channel: i as u64,
                                    amount: amount.into(),
                                };

                                let mut c = shared_c.access_consumer().await;
                                frame.encode(&mut c).await?
                            }
                            Ok(Right(())) => {
                                let mut c = shared_c.access_consumer().await;
                                c.close(()).await?;
                                return Result::<(), Option<C::Error>>::Ok(());
                            }
                        }
                    }
                }
            },
        ))
        .map_ok(|_| ());

        let give_all_guarantees = try_join_all(guarantees_to_gives.into_iter().enumerate().map(
            |(i, mut guarantees_to_give)| {
                let shared_c = self.state.c.clone();

                async move {
                    loop {
                        match guarantees_to_give.produce().await {
                            Err(()) => return Err(None),
                            Ok(Left(amount)) => {
                                let frame = IssueGuarantee {
                                    channel: i as u64,
                                    amount,
                                };

                                let mut c = shared_c.access_consumer().await;
                                frame.encode(&mut c).await?
                            }
                            Ok(Right(())) => {
                                let mut c = shared_c.access_consumer().await;
                                c.close(()).await?;
                                return Result::<(), Option<C::Error>>::Ok(());
                            }
                        }
                    }
                }
            },
        ))
        .map_ok(|_| ());

        let start_all_dropping = try_join_all(start_droppings.into_iter().enumerate().map(
            |(i, mut start_droppings)| {
                let shared_c = self.state.c.clone();

                async move {
                    loop {
                        match start_droppings.produce().await {
                            Err(()) => return Err(None),
                            Ok(Left(())) => {
                                let frame = AnnounceDropping { channel: i as u64 };

                                let mut c = shared_c.access_consumer().await;
                                frame.encode(&mut c).await?
                            }
                            Ok(Right(())) => {
                                let mut c = shared_c.access_consumer().await;
                                c.close(()).await?;
                                return Result::<(), Option<C::Error>>::Ok(());
                            }
                        }
                    }
                }
            },
        ))
        .map_ok(|_| ());

        try_join3(
            grant_all_absolutions,
            give_all_guarantees,
            start_all_dropping,
        )
        .map_ok(|_| ())
        .await
    }
}

/// Send global messages to the other peer via this struct.
#[derive(Debug)]
pub struct GlobalMessageSender<const NUM_CHANNELS: usize, R> {
    state: R,
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> GlobalMessageSender<NUM_CHANNELS, R>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer<Item = u8, Final = (), Error: Clone> + BulkConsumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    pub async fn send_global_message<CMessage: EncodableKnownSize>(
        &mut self,
        message: CMessage,
    ) -> Result<(), C::Error> {
        let mut c = self.state.deref().c.access_consumer().await;

        let header = SendGlobalHeader {
            length: message.len_of_encoding() as u64,
        };
        header.encode(&mut c).await?;

        message.encode(&mut c).await
    }
}

/// Receive global messenges from the other peer via the [`Producer`] implementation of this struct. The producer must constantly be read, as it internally decodes and processes all control messages.
#[derive(Debug)]
pub struct GlobalMessageReceiver<
    const NUM_CHANNELS: usize,
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    C: Consumer,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    GlobalMessage,
    GlobalMessageDecodeErrorReason,
> {
    state: R,
    client_receivers: [client_logic::MessageReceiver<
        ProjectIthClientState<NUM_CHANNELS, R, P, PR, C, CR>,
    >; NUM_CHANNELS],
    server_receivers: [server_logic::MessageReceiver<
        ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>,
        Fixed<u8>,
    >; NUM_CHANNELS],
    urgent_channels: [bool; NUM_CHANNELS],
    phantom: PhantomData<(GlobalMessage, GlobalMessageDecodeErrorReason)>,
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR, GlobalMessage, GlobalMessageDecodeErrorReason>
    Producer
    for GlobalMessageReceiver<
        NUM_CHANNELS,
        R,
        P,
        PR,
        C,
        CR,
        GlobalMessage,
        GlobalMessageDecodeErrorReason,
    >
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: BulkProducer<Item = u8, Final: Clone, Error: Clone>,
    C: Consumer<Item = u8, Final = (), Error: Clone> + BulkConsumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    GlobalMessage: Decodable<ErrorReason = GlobalMessageDecodeErrorReason>,
{
    type Item = GlobalMessage;

    type Final = P::Final;

    type Error = GlobalMessageError<P::Final, P::Error, GlobalMessageDecodeErrorReason>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        let mut p = self.state.p.access_producer().await;

        // Decode frames until encountering a `SendGlobal` frame header. For all other frames, update the corresponding client or server state.
        loop {
            // Is the producer done? If so, emit final.
            match p.expose_items().await? {
                Left(_) => {} // no-op
                Right(fin) => return Ok(Right(fin)),
            }

            match IncomingFrameHeader::decode(&mut p).await.map_err(
                |decode_err| match decode_err {
                    DecodeError::Producer(err) => err.into(),
                    _ => GlobalMessageError::Other,
                },
            )? {
                // Forward incoming IssueGuarantee frames to the indicated client input.
                IncomingFrameHeader::IssueGuarantee(IssueGuarantee { channel, amount }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.client_receivers[channel as usize]
                            .receive_guarantees(amount)
                            .or(Err(GlobalMessageError::Other))?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // Forward incoming Absolve frames to the indicated server input.
                IncomingFrameHeader::Absolve(Absolve { channel, amount }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_receivers[channel as usize]
                            .receive_absolve(amount)
                            .or(Err(GlobalMessageError::Other))?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // Forward incoming Plead frames to the indicated client input.
                IncomingFrameHeader::Plead(Plead { channel, target }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        if let Some(target_nz) = NonZeroU64::new(target) {
                            self.client_receivers[channel as usize].receive_plead(target_nz);
                        }
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // Forward incoming LimitSending frames to the indicated server input.
                IncomingFrameHeader::LimitSending(LimitSending { channel, bound }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_receivers[channel as usize]
                            .receive_limit_sending(bound)
                            .or(Err(GlobalMessageError::Other))?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // Forward incoming LimitReceiving frames to the indicated client input.
                IncomingFrameHeader::LimitReceiving(LimitReceiving { channel, bound }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.client_receivers[channel as usize]
                            .receive_limit_receiving(bound)
                            .or(Err(GlobalMessageError::Other))?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // AnnounceDropping frames should never arrive, because we do not send optimistically.
                IncomingFrameHeader::AnnounceDropping(AnnounceDropping { channel: _ }) => {
                    return Err(GlobalMessageError::Other);
                }

                // Forward incoming Apologise frames to the indicated server input.
                IncomingFrameHeader::Apologise(Apologise { channel }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        self.server_receivers[channel as usize]
                            .receive_apologise()
                            .or(Err(GlobalMessageError::Other))?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // When receiving a SendToChannel frame header, forward the indicated amount of bytes into the corresponding buffer.
                IncomingFrameHeader::SendToChannelHeader(SendToChannelHeader {
                    channel,
                    length,
                }) => {
                    if channel < (NUM_CHANNELS as u64) {
                        let server_receiver = &mut self.server_receivers[channel as usize];

                        if server_receiver.is_buffer_empty()
                            && self.urgent_channels[channel as usize]
                        {
                            let old_count = self.state.number_of_nonempty_urgent_channels.get();
                            self.state
                                .number_of_nonempty_urgent_channels
                                .set(old_count + 1);
                            let _ = self.state.no_urgency_notifier.try_take();
                        }

                        server_receiver
                            .receive_send_to_channel(length, &mut p)
                            .await
                            .map_err(|err| match err {
                                ReceiveSendToChannelError::ProducerError(err) => {
                                    GlobalMessageError::Producer(err)
                                }
                                _ => GlobalMessageError::Other,
                            })?;
                        continue;
                    } else {
                        return Err(GlobalMessageError::UnsupportedChannel(channel));
                    }
                }

                // When receiving a SendControl frame header, we can actually produce an item! Yay!
                IncomingFrameHeader::SendGlobalHeader(SendGlobalHeader { length: _ }) => {
                    // Did the producer end?
                    match p.expose_items().await? {
                        Left(_) => {} // no-op
                        Right(fin) => return Ok(Right(fin)),
                    }

                    return Ok(Left(GlobalMessage::decode(&mut p).await?));
                }
            }
        }
    }
}

/// Reasons why producing an item from a [`GlobalMessageReceiver`] can fail: the underlying transport may fail, the peer might send a global message that cannot be decoded, the peer might reference a channel we did not expect, or the peer might deviate from the LCMUX spec in any other way.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum GlobalMessageError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason> {
    /// The underlying transport errored.
    Producer(ProducerError),
    /// The peer sent a `SendGlobal` frame, but its payload did not decode correctly.
    DecodeGlobalMessage(DecodeError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>),
    /// The peer references a channel_id which we did not expect.
    UnsupportedChannel(u64),
    /// The peer deviated from the LCMUX specification.
    Other,
}

impl<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason> From<ProducerError>
    for GlobalMessageError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>
{
    fn from(err: ProducerError) -> Self {
        GlobalMessageError::Producer(err)
    }
}

impl<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>
    From<DecodeError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>>
    for GlobalMessageError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>
{
    fn from(
        err: DecodeError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>,
    ) -> Self {
        GlobalMessageError::DecodeGlobalMessage(err)
    }
}

impl<ProducerFinal: Display, ProducerError: Display, GlobalMessageDecodeErrorReason: Display>
    Display for GlobalMessageError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            GlobalMessageError::Producer(err) => {
                write!(f, "The underlying producer encountered an error: {}", err,)
            }
            GlobalMessageError::DecodeGlobalMessage(fin) => {
                write!(f, "Decoding an LCMUX global message failed: {}", fin)
            }
            GlobalMessageError::UnsupportedChannel(channel_id) => {
                write!(
                    f,
                    "Peer used a logical channel id we did not expect: {}",
                    channel_id
                )
            }
            GlobalMessageError::Other => {
                write!(f, "Peer deviated from the LCMUX spec")
            }
        }
    }
}

impl<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason> Error
    for GlobalMessageError<ProducerFinal, ProducerError, GlobalMessageDecodeErrorReason>
where
    ProducerFinal: Display + Debug,
    ProducerError: 'static + Error,
    GlobalMessageDecodeErrorReason: 'static + Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GlobalMessageError::Producer(err) => Some(err),
            GlobalMessageError::DecodeGlobalMessage(_)
            | GlobalMessageError::UnsupportedChannel(_)
            | GlobalMessageError::Other => None,
        }
    }
}

#[derive(Debug)]
struct ProjectIthClientState<const NUM_CHANNELS: usize, R, P, PR, C, CR> {
    r: R,
    i: usize,
    phantom: PhantomData<(P, PR, C, CR)>,
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Deref
    for ProjectIthClientState<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    type Target = client_logic::State;

    fn deref(&self) -> &Self::Target {
        &self.r.deref().channel_states[self.i].0
    }
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Clone
    for ProjectIthClientState<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Clone,
{
    fn clone(&self) -> Self {
        Self {
            r: self.r.clone(),
            i: self.i,
            phantom: self.phantom,
        }
    }
}

#[derive(Debug)]
struct ProjectIthServerState<const NUM_CHANNELS: usize, R, P, PR, C, CR> {
    r: R,
    i: usize,
    phantom: PhantomData<(P, PR, C, CR)>,
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Deref
    for ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Deref<Target = State<NUM_CHANNELS, P, PR, C, CR>>,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    type Target = server_logic::State<Fixed<u8>>;

    fn deref(&self) -> &Self::Target {
        &self.r.deref().channel_states[self.i].1
    }
}

impl<const NUM_CHANNELS: usize, R, P, PR, C, CR> Clone
    for ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>
where
    R: Clone,
{
    fn clone(&self) -> Self {
        Self {
            r: self.r.clone(),
            i: self.i,
            phantom: self.phantom,
        }
    }
}
