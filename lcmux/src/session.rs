use std::{
    error::Error,
    fmt::{Debug, Display, Formatter},
    marker::PhantomData,
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
    Decodable, DecodeError, Encodable, EncodableKnownSize, RelativeDecodable, RelativeEncodable,
};
use ufotofu_queues::Fixed;
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
};

use crate::{
    client_logic::{self, LogicalChannelClientError},
    frames::*,
    server_logic::{self, ReceiveSendToChannelError},
};

pub use crate::frames::SendGlobalNibble;

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
    /// The `watermark` specifies how many guarantees must become granatble at least before they are actually granted.
    pub fn new(
        p: SharedProducer<PR, P>,
        c: SharedConsumer<CR, C>,
        buffer_capacity: usize,
        watermark: u64,
    ) -> Self {
        Self {
            channel_states: core::array::from_fn(|i| {
                (
                    client_logic::State::new(i as u64),
                    server_logic::State::new(
                        i as u64,
                        Fixed::new(buffer_capacity),
                        buffer_capacity,
                        watermark,
                    ),
                )
            }),
            p,
            c,
        }
    }
}

/// All components for interacting with an LCMUX session.
///
/// `NUM_CHANNELS` is the number of channels. `R` is the type by which to access the corresponding [`State`].
/// `P` is the type of the producer of the underlying communication channel, and `PR` is the type of references to the shared state of the `SharedProducer<P>`.
/// `C` is the type of the consumer of the underlying communication channel, and `CR` is the type of references to the shared state of the `SharedConsumer<C>`.
#[derive(Debug)]
pub struct Session<
    const NUM_CHANNELS: usize,
    R,
    P,
    PR,
    C,
    CR,
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
{
    /// Creates a new LCMUX session.
    pub fn new(state_ref: R) -> Self {
        todo!()
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
        let mut consumer = self.0.state.r.c.clone();
        self.0.send_to_channel(&mut consumer, message).await
    }

    /// Send a `LimitSending` frame to the server.
    ///
    /// This method does not check that you send valid limits or that you respect them on the future.
    pub async fn limit_sending(&mut self, bound: u64) -> Result<(), C::Error> {
        let mut consumer = self.0.state.r.c.clone();
        self.0.limit_sending(&mut consumer, bound).await
    }

    /// Same as `self.limit_sending(0)`.
    pub async fn close(&mut self) -> Result<(), C::Error> {
        self.limit_sending(0).await
    }
}

#[derive(Debug)]
pub struct ChannelReceiver<const NUM_CHANNELS: usize, R, P, PR, C, CR>(
    server_logic::ReceivedData<ProjectIthServerState<NUM_CHANNELS, R, P, PR, C, CR>, Fixed<u8>>,
);

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
        self.0.produce().await
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
        self.0.slurp().await
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
        self.0.expose_items().await
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.0.consider_produced(amount).await
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
    pub async fn send_global_message<
        CMessage: RelativeEncodable<SendGlobalNibble> + GetGlobalNibble,
    >(
        &mut self,
        message: CMessage,
    ) -> Result<(), C::Error> {
        let mut c = self.state.deref().c.access_consumer().await;

        let nibble = message.control_nibble();
        let header = SendGlobalHeader {
            encoding_nibble: nibble,
        };
        header.encode(&mut c).await?;

        message.relative_encode(&mut c, &nibble).await?;
        c.close(()).await
    }
}

/// A trait for encoding control messages: given a reference to a control message, yields the correspondong [`SendControlNibble`].
pub trait GetGlobalNibble {
    fn control_nibble(&self) -> SendGlobalNibble;
}

/// Receive global messenges from the other peer via the [`Producer`] implementation of this struct. The producer must constantly be read, as it internally decodes and processes all control messages.
#[derive(Debug)]
pub struct GlobalMessageReceiver<
    const NUM_CHANNELS: usize,
    R,
    P,
    PR,
    C,
    CR,
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
    GlobalMessage: RelativeDecodable<SendGlobalNibble, GlobalMessageDecodeErrorReason>,
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
                        self.server_receivers[channel as usize]
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
                IncomingFrameHeader::SendGlobalHeader(SendGlobalHeader { encoding_nibble }) => {
                    // Did the producer end?
                    match p.expose_items().await? {
                        Left(_) => {} // no-op
                        Right(fin) => return Ok(Right(fin)),
                    }

                    return Ok(Left(
                        GlobalMessage::relative_decode(&mut p, &encoding_nibble).await?,
                    ));
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
