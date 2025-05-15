use std::future::Future;

use futures::try_join;
use lcmux::{ChannelOptions, InitOptions, Session};
use max_payload_size_handling::{
    ConsumerButFirstSendByte, ConsumerButFirstSetReceivedMaxPayloadSize,
};
//use lcmux::new_lcmux;
use ufotofu::{BulkConsumer, BulkProducer, Producer};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    OnceCell,
};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    Store, StoreEvent, SubspaceId,
};

mod parameters;

mod messages;
use messages::*;

mod max_payload_size_handling;

use ufotofu_codec::{Blame, DecodableCanonic, DecodeError, EncodableKnownSize, EncodableSync};
use willow_transport_encryption::{
    parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing},
    run_handshake, DecryptionError, EncryptionError, HandshakeError,
};

mod data_handles;

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

pub struct SyncOptions<'prologue, DH: DiffieHellmanSecretKey> {
    pub max_payload_power: u8,
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
}

pub async fn sync_with_peer<
    'prologue,
    const HASHLEN_IN_BYTES: usize,
    const BLOCKLEN_IN_BYTES: usize,
    const PK_ENCODING_LENGTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_2: usize,
    const TAG_WIDTH_IN_BYTES_PLUS_4096: usize,
    const NONCE_WIDTH_IN_BYTES: usize,
    const IS_TAG_PREPENDED: bool,
    H,
    DH,
    AEAD,
    E,
    C,
    P,
>(
    options: &SyncOptions<'prologue, DH>,
    consumer: C,
    producer: P,
) -> Result<(), WgpsError<E>>
where
    DH: DiffieHellmanSecretKey + Clone,
    DH::PublicKey: Default + EncodableSync + EncodableKnownSize + DecodableCanonic + Clone,
    AEAD: Default,
    H: Hashing<HASHLEN_IN_BYTES, BLOCKLEN_IN_BYTES, AEAD>,
    AEAD: AEADEncryptionKey<TAG_WIDTH_IN_BYTES, NONCE_WIDTH_IN_BYTES, IS_TAG_PREPENDED>,
    E: Clone,
    C: BulkConsumer<Item = u8, Final = (), Error = E>,
    P: BulkProducer<Item = u8, Final = (), Error = E>,
{
    let (salt_base, peer_capability_receiver, c, p) = run_handshake::<
        HASHLEN_IN_BYTES,
        BLOCKLEN_IN_BYTES,
        PK_ENCODING_LENGTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES,
        TAG_WIDTH_IN_BYTES_PLUS_2,
        TAG_WIDTH_IN_BYTES_PLUS_4096,
        NONCE_WIDTH_IN_BYTES,
        IS_TAG_PREPENDED,
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

    let (our_salt, their_salt) = if options.is_initiator {
        (salt_base, invert_bytes(salt_base))
    } else {
        (invert_bytes(salt_base), salt_base)
    };

    // This is set to the max payload size received from the peer once it has arrived.
    let received_max_payload_size = OnceCell::<u64>::new();

    let consumer = ConsumerButFirstSendByte::new(options.max_payload_power, c);
    let producer = ConsumerButFirstSetReceivedMaxPayloadSize::new(&received_max_payload_size, p);

    let shared_consumer_state = shared_consumer::State::new(consumer);
    let consumer = SharedConsumer::new(&shared_consumer_state);

    let shared_producer_state = shared_producer::State::new(producer);
    let producer = SharedProducer::new(&shared_producer_state);

    let lcmux_state = lcmux::State::<4, _, _, _, _>::new(
        producer,
        consumer,
        [
            &options.reconciliation_channel_options,
            &options.data_channel_options,
            &options.overlap_channel_options,
            &options.capability_channel_options,
        ],
    );

    let mut lcmux_session: Session<4, _, _, _, _, _, GlobalMessage, Blame> =
        Session::new(&lcmux_state, [false, false, true, true]);

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
        global_receiver,
        channel_senders,
        channel_receivers,
    } = lcmux_session;

    // Every unit of work that the WGPS needs to perform is defined as a future in the following, via an async block.

    let do_the_bookkeeping = async {
        bookkeeping
            .keep_the_books()
            .await
            .map_err::<WgpsError<E>, _>(|err| match err {
                Some(EncryptionError::Inner(channel_err)) => WgpsError::from(channel_err),
                Some(EncryptionError::NoncesExhausted) => WgpsError::NonceExhausted,
                None => WgpsError::PeerMisbehaved,
            })
    };

    // Actually run all the concurrent operations of the WGPS.
    let _ = try_join!(do_the_bookkeeping)?; // TODO add futures for the remaining, as of yet unimplemented tasks: rbsr, pai, etc. Will be fleshed out as we progress with the WGPS implementation.

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
