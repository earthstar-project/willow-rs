use std::{future::Future, rc::Rc};

use either::Either::{Left, Right};
use futures::try_join;
use lcmux::{ChannelOptions, GlobalMessageError, InitOptions, Session};
use ufotofu::{BulkConsumer, BulkProducer, Consumer, Producer};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    spsc, Mutex,
};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    Store, StoreEvent, SubspaceId,
};

mod parameters;

pub mod messages;
use messages::*;

use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, EncodableKnownSize, EncodableSync,
    RelativeEncodableKnownSize,
};
use willow_pio::PrivateInterest;
use willow_transport_encryption::{
    parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing},
    run_handshake, DecryptionError, Decryptor, EncryptionError, Encryptor, HandshakeError,
};

pub mod data_handles;
mod pio;

use crate::{
    parameters::{
        EnumerationCapability, ReadCapability, WgpsEnumerationCapability, WgpsNamespaceId,
        WgpsReadCapability, WgpsSubspaceId,
    },
    pio::PioSession,
};

/// An error which can occur during a WGPS synchronisation session.
pub enum WgpsError<E> {
    /// The handshake went wrong.
    Handshake(HandshakeError<E, (), E>),
    /// The underlying transport emitted an error.
    Transport(E),
    /// The peer sent an invalid first byte, not indicating a valid max payload size.
    InvalidMaxPayloadSize,
    /// The peer deviated from the WGPS in some fatal form.
    PeerMisbehaved,
    /// Sent or received more than 2^64 noise transport messages.
    NonceExhausted,
    /// Received invalid cyphertexts or encryption framing data.
    InvalidEncryption,
    /// The incoming data stream ended but the end was not cryptographically authenticated. A malicious actor may have cut things off.
    UnauthenticatedEndOfStream,
}

impl<E> From<E> for WgpsError<E> {
    fn from(value: E) -> Self {
        Self::Transport(value)
    }
}

impl<E> From<EncryptionError<E>> for WgpsError<E> {
    fn from(value: EncryptionError<E>) -> Self {
        match value {
            EncryptionError::Inner(err) => WgpsError::Transport(err),
            EncryptionError::NoncesExhausted => WgpsError::NonceExhausted,
        }
    }
}

impl<E> From<DecryptionError<E>> for WgpsError<E> {
    fn from(value: DecryptionError<E>) -> Self {
        match value {
            DecryptionError::Inner(err) => WgpsError::Transport(err),
            DecryptionError::NoncesExhausted => WgpsError::NonceExhausted,
            DecryptionError::DecryptionFailure
            | DecryptionError::WeirdEndOfStream
            | DecryptionError::InvalidHeader => WgpsError::InvalidEncryption,
            DecryptionError::UnauthenticatedEndOfStream => WgpsError::UnauthenticatedEndOfStream,
        }
    }
}

impl<E> From<HandshakeError<E, (), E>> for WgpsError<E> {
    fn from(value: HandshakeError<E, (), E>) -> Self {
        Self::Handshake(value)
    }
}

impl<E> From<GlobalMessageError<(), DecryptionError<E>, Blame>> for WgpsError<E> {
    fn from(value: GlobalMessageError<(), DecryptionError<E>, Blame>) -> Self {
        match value {
            GlobalMessageError::Producer(err) => Self::from(err),
            GlobalMessageError::DecodeGlobalMessage(_)
            | GlobalMessageError::UnsupportedChannel(_)
            | GlobalMessageError::Other => Self::PeerMisbehaved,
        }
    }
}

impl<E: core::fmt::Display> core::fmt::Display for WgpsError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WgpsError::Transport(transport_error) => write!(f, "{}", transport_error),
            WgpsError::Handshake(hs_error) => match hs_error {
                HandshakeError::SendingError(transport_error) => write!(f, "{}", transport_error),
                HandshakeError::ReceivingError(DecodeError::UnexpectedEndOfInput(())) => write!(
                    f,
                    "The peer terminated the transport channel during the handshake."
                ),
                HandshakeError::ReceivingError(DecodeError::Producer(err)) => write!(f, "{}", err),
                HandshakeError::ReceivingError(DecodeError::Other(err)) => write!(f, "{}", err),
            },
            WgpsError::InvalidMaxPayloadSize => write!(
                f,
                "The peer sent an invalid first byte, not indicating a valid max payload size."
            ),
            WgpsError::PeerMisbehaved => {
                write!(f, "The peer deviated from the WGPS in some fatal form.")
            }
            WgpsError::NonceExhausted => {
                write!(
                    f,
                    "We sent or received more than 2^64 noise transport messages."
                )
            }
            WgpsError::InvalidEncryption => {
                write!(
                    f,
                    "The peer sent invalid cyphertexts or encryption framing data."
                )
            }
            WgpsError::UnauthenticatedEndOfStream => {
                write!(f, "The incoming data stream ended, but its end was not cryptographically authenticated. A malicious actor may have cut things off.")
            }
        }
    }
}

pub struct SyncOptions<
    'prologue,
    const HANDSHAKE_HASHLEN_IN_BYTES: usize,
    const PIO_INTEREST_HASH_LENGTH_IN_BYTES: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    DH: DiffieHellmanSecretKey,
    N,
    S,
> {
    /* Handshake Options */
    pub is_initiator: bool,
    pub esk: DH,
    pub epk: DH::PublicKey,
    pub ssk: DH,
    pub spk: DH::PublicKey,
    pub protocol_name: &'static [u8],
    pub prologue: &'prologue [u8],
    /* End of Handshake Options */
    /* LCMUX channel options */
    pub reconciliation_channel_options: ChannelOptions,
    pub data_channel_options: ChannelOptions,
    pub overlap_channel_options: ChannelOptions,
    pub capability_channel_options: ChannelOptions,
    /// How many message bytes to receive at most on the overlap channel.
    pub overlap_channel_receiving_limit: u64,
    /// How many message bytes to receive at most on the capability channel.
    pub capability_channel_receiving_limit: u64,
    /* End of LCMUX Channel Options */
    /* Private Interest Overlap Options */
    hash_private_interest: fn(
        &PrivateInterest<MCL, MCC, MPL, N, S>,
        &[u8; HANDSHAKE_HASHLEN_IN_BYTES],
    ) -> [u8; PIO_INTEREST_HASH_LENGTH_IN_BYTES],
    /* End of Private Interest Overlap Options */
}

pub async fn sync_with_peer<
    'prologue,
    const HANDSHAKE_HASHLEN_IN_BYTES: usize, // This is also the PIO SALT_LENGTH
    const HANDSHAKE_BLOCKLEN_IN_BYTES: usize,
    const HANDSHAKE_PK_ENCODING_LENGTH_IN_BYTES: usize,
    const HANDSHAKE_TAG_WIDTH_IN_BYTES: usize,
    const HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_2: usize,
    const HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
    const HANDSHAKE_NONCE_WIDTH_IN_BYTES: usize,
    const HANDSHAKE_IS_TAG_PREPENDED: bool,
    const PIO_INTEREST_HASH_LENGTH_IN_BYTES: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    H,
    DH,
    AEAD,
    E,
    C,
    P,
    N,
    S,
    MyReadCap,
    MyEnumCap,
    TheirReadCap,
    TheirEnumCap,
>(
    options: &SyncOptions<
        'prologue,
        HANDSHAKE_HASHLEN_IN_BYTES,
        PIO_INTEREST_HASH_LENGTH_IN_BYTES,
        MCL,
        MCC,
        MPL,
        DH,
        N,
        S,
    >,
    consumer: C,
    producer: P,
) -> Result<(), WgpsError<E>>
where
    DH: DiffieHellmanSecretKey<PublicKey = S> + Clone,
    AEAD: Default,
    H: Hashing<HANDSHAKE_HASHLEN_IN_BYTES, HANDSHAKE_BLOCKLEN_IN_BYTES, AEAD>,
    AEAD: AEADEncryptionKey<
            HANDSHAKE_TAG_WIDTH_IN_BYTES,
            HANDSHAKE_NONCE_WIDTH_IN_BYTES,
            HANDSHAKE_IS_TAG_PREPENDED,
        > + 'static,
    E: Clone + 'static,
    C: BulkConsumer<Item = u8, Final = (), Error = E> + 'static,
    P: BulkProducer<Item = u8, Final = (), Error = E> + 'static,
    N: WgpsNamespaceId,
    S: WgpsSubspaceId,
    MyReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S> + 'static,
    MyEnumCap: EnumerationCapability<NamespaceId = N, Receiver = S> + 'static,
    TheirReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    TheirEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
{
    //////////////////////
    // Handshake Things //
    //////////////////////

    let (salt_base, peer_capability_receiver, c, p) = run_handshake::<
        HANDSHAKE_HASHLEN_IN_BYTES,
        HANDSHAKE_BLOCKLEN_IN_BYTES,
        HANDSHAKE_PK_ENCODING_LENGTH_IN_BYTES,
        HANDSHAKE_TAG_WIDTH_IN_BYTES,
        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_2,
        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_4096,
        HANDSHAKE_NONCE_WIDTH_IN_BYTES,
        HANDSHAKE_IS_TAG_PREPENDED,
        H,
        DH,
        AEAD,
        C,
        P,
    >(
        options.is_initiator,
        options.esk.clone(),
        options.epk.clone(),
        options.ssk.clone(),
        options.spk.clone(),
        options.protocol_name,
        options.prologue,
        consumer,
        producer,
    )
    .await?;

    let (my_salt, their_salt) = if options.is_initiator {
        (salt_base, invert_bytes(salt_base))
    } else {
        (invert_bytes(salt_base), salt_base)
    };

    //////////////////
    // LCMUX Things //
    //////////////////

    let shared_consumer_state = shared_consumer::State::new(c);
    let consumer = SharedConsumer::new(Rc::new(shared_consumer_state));

    let shared_producer_state = shared_producer::State::new(p);
    let producer = SharedProducer::new(Rc::new(shared_producer_state));

    let mut lcmux_session: Session<
        4,
        _,
        _,
        _,
        _,
        _,
        GlobalMessage<PIO_INTEREST_HASH_LENGTH_IN_BYTES, TheirEnumCap>,
        Blame,
        Option<
            Rc<
                pio::State<
                    HANDSHAKE_HASHLEN_IN_BYTES,
                    PIO_INTEREST_HASH_LENGTH_IN_BYTES,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    MyReadCap,
                    MyEnumCap,
                    Decryptor<
                        HANDSHAKE_TAG_WIDTH_IN_BYTES,
                        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_2,
                        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_4096,
                        HANDSHAKE_NONCE_WIDTH_IN_BYTES,
                        HANDSHAKE_IS_TAG_PREPENDED,
                        AEAD,
                        P,
                    >,
                    (),
                    DecryptionError<E>,
                    Encryptor<
                        HANDSHAKE_TAG_WIDTH_IN_BYTES,
                        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_2,
                        HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_4096,
                        HANDSHAKE_NONCE_WIDTH_IN_BYTES,
                        HANDSHAKE_IS_TAG_PREPENDED,
                        AEAD,
                        C,
                    >,
                    EncryptionError<E>,
                >,
            >,
        >,
    > = Session::new(
        producer,
        consumer,
        [
            &options.reconciliation_channel_options,
            &options.data_channel_options,
            &options.overlap_channel_options,
            &options.capability_channel_options,
        ],
        [false, false, true, true],
        None,
    );

    lcmux_session
        .init([
            InitOptions {
                receiving_limit: None,
            },
            InitOptions {
                receiving_limit: None,
            },
            InitOptions {
                receiving_limit: Some(options.overlap_channel_receiving_limit),
            },
            InitOptions {
                receiving_limit: Some(options.capability_channel_receiving_limit),
            },
        ])
        .await?;

    let Session {
        mut bookkeeping,
        global_sender,
        mut global_receiver,
        channel_senders,
        channel_receivers,
    } = lcmux_session;

    let [reconciliation_channel_sender, data_channel_sender, overlap_channel_sender, capability_channel_sender] =
        channel_senders;
    let [reconciliation_channel_receiver, data_channel_receiver, overlap_channel_receiver, capability_channel_receiver] =
        channel_receivers;

    let global_sender = Mutex::new(global_sender);

    ////////////////
    // PIO Things //
    ////////////////

    let announce_overlap_in_memory_channel_state = Rc::new(spsc::State::new(
        ufotofu_queues::Fixed::new_with_manual_tmps(
            8,
            PioAnnounceOverlap::<PIO_INTEREST_HASH_LENGTH_IN_BYTES, TheirEnumCap>::temporary,
        ),
    ));
    let (mut announce_overlap_in_memory_sender, announce_overlap_in_memory_receiver) =
        spsc::new_spsc(announce_overlap_in_memory_channel_state);

    let PioSession {
        my_aoi_input,
        overlap_output,
        state: pio_state,
    }: PioSession<
        HANDSHAKE_HASHLEN_IN_BYTES,
        PIO_INTEREST_HASH_LENGTH_IN_BYTES,
        MCL,
        MCC,
        MPL,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        _,
        _, //P::Final,
        _, //P::Error,
        _,
        _, //C::Error,
        _,
    > = PioSession::new(
        my_salt,
        options.hash_private_interest,
        options.spk.clone(),
        peer_capability_receiver,
        global_sender,
        capability_channel_sender,
        capability_channel_receiver,
        overlap_channel_sender,
        overlap_channel_receiver,
        announce_overlap_in_memory_receiver,
    );

    *global_receiver.get_relative_to_mut() = Some(pio_state);

    // Every unit of work that the WGPS needs to perform is defined as a future in what follows, via an async block.

    let do_the_lcmux_bookkeeping = async {
        bookkeeping
            .keep_the_books()
            .await
            .map_err::<WgpsError<E>, _>(|err| match err {
                Some(EncryptionError::Inner(channel_err)) => WgpsError::from(channel_err),
                Some(EncryptionError::NoncesExhausted) => WgpsError::NonceExhausted,
                None => WgpsError::PeerMisbehaved,
            })
    };

    let receive_global_messages = async {
        loop {
            match global_receiver.produce().await? {
                Left(GlobalMessage::PioAnnounceOverlap(msg)) => {
                    // We have an in-memory-channel into which to enqueue all PioAnnounceOverlap messages we receive.
                    // The PIO implementation then does the remainder of the work.
                    announce_overlap_in_memory_sender.consume(msg).await.unwrap(/* infallible */);
                }
                Right(fin) => {
                    // We won't receive more global messages from the other peer. That does not necessarily terminate reconciliation, but we *can* stop polling for *more* global messages.
                    return Ok(());
                }
                _ => todo!(),
            }
        }
    };

    // Actually run all the concurrent operations of the WGPS.
    let _ = try_join!(do_the_lcmux_bookkeeping, receive_global_messages)?; // TODO add futures for the remaining, as of yet unimplemented tasks: rbsr, pai, etc. Will be fleshed out as we progress with the WGPS implementation.

    Ok(())
}

fn invert_bytes<const LEN: usize>(mut bytes: [u8; LEN]) -> [u8; LEN] {
    for byte in bytes.iter_mut() {
        *byte ^= 255;
    }

    bytes
}

/// Options to specify how ranges should be partitioned.
#[derive(Debug, Clone, Copy)]
pub struct PartitionOpts {
    /// The largest number of entries that can be included by a range before it is better to send that range's fingerprint instead of sending its entries.
    pub max_range_size: usize,
    /// The maximum number of partitions to split a range into. Must be at least 2.
    pub max_splits: usize,
}

/// A split range and the action which should be taken with that split range during range-based set reconciliation.
pub type RangeSplit<const MCL: usize, const MCC: usize, const MPL: usize, S, FP> =
    (Range3d<MCL, MCC, MPL, S>, SplitAction<FP>);

/// Whether to send a split range's fingerprint or its included entries.
#[derive(Debug)]
pub enum SplitAction<FP> {
    SendFingerprint(FP),
    SendEntries(u64),
}

/// A [`Store`] capable of performing [3d range-based set reconciliation](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3_range_based_set_reconciliation).
pub trait RbsrStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, FP>
where
    Self: Store<MCL, MCC, MPL, N, S, PD, AT>,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Query which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a [`Range3d`], returning a producer of [`LengthyAuthorisedEntry`].
    ///
    /// If `will_sort` is `true`, entries will be sorted ascendingly by subspace_id first, path second.
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn query_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        will_sort: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Subscribe to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Range3d`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn subscribe_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Summarise a [`Range3d`] as a [fingerprint](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3rbsr_fp).
    fn summarise(&self, range: Range3d<MCL, MCC, MPL, S>) -> impl Future<Output = FP>;

    /// Convert an [`AreaOfInterest`] to a concrete [`Range3d`] including all the entries the given [`AreaOfInterest`] would.
    fn area_of_interest_to_range(
        &self,
        aoi: &AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> impl Future<Output = Range3d<MCL, MCC, MPL, S>>;

    /// Partition a [`Range3d`] into many parts, or return `None` if the given range cannot be split (for instance because the range only includes a single entry).
    fn partition_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        options: &PartitionOpts,
    ) -> impl Future<Output = Option<impl Iterator<Item = RangeSplit<MCL, MCC, MPL, S, FP>>>>;
}
