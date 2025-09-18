use std::{cell::Cell, collections::HashMap, future::Future, hash::Hash, pin::Pin, rc::Rc};

use either::Either::{self, Left, Right};
use futures::{try_join, SinkExt, StreamExt};
use lcmux::{ChannelOptions, GlobalMessageError, InitOptions, Session};
use ufotofu::{BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    spsc, Mutex, TakeCell,
};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, AuthorisedEntry, Entry, EntryIngestionError, EntryOrigin,
    LengthyAuthorisedEntry, LengthyEntry, NamespaceId, PayloadAppendError, PayloadDigest,
    QueryIgnoreParams, Store, StoreEvent, SubspaceId,
};

pub mod parameters;

mod rbsr;
mod storedinator;

pub mod messages;
use messages::*;

use ufotofu_codec::{Blame, DecodeError, RelativeDecodable, RelativeEncodableKnownSize};
use willow_pio::{PersonalPrivateInterest, PrivateInterest};
use willow_transport_encryption::{
    parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing},
    run_handshake, DecryptionError, Decryptor, EncryptionError, Encryptor, HandshakeError,
};

pub mod data_handles;
mod pio;

use crate::{
    parameters::{
        EnumerationCapability, ReadCapability, WgpsAuthorisationToken, WgpsEnumerationCapability,
        WgpsFingerprint, WgpsNamespaceId, WgpsPayloadDigest, WgpsReadCapability, WgpsSubspaceId,
    },
    pio::{AoiInputError, AoiOutputError, CapableAoi, MyAoiInput, PioSession},
    rbsr::{order_entries, process_subscription, RbsrError, ReconciliationSender},
    storedinator::Storedinator,
};

/// An error which can occur during a WGPS synchronisation session.
pub enum WgpsError<E, StoreCreationError, StoreError> {
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
    /// We were supposed to transmit the enumeration capability for a read capability of ours, but none was available.
    NoEnumerationCapability,
    /// Creating a new store (for a new namespace) went wrong.
    StoreCreation(StoreCreationError),
    /// A data store encountered a fatal error.
    Store(StoreError),
    /// We received valid data, but could not process it due to limitations of our own (e.g. running out of memory).
    OurFault,
}

impl<E, StoreCreationError, StoreError> From<E> for WgpsError<E, StoreCreationError, StoreError> {
    fn from(value: E) -> Self {
        Self::Transport(value)
    }
}

impl<E, StoreCreationError, StoreError> From<EncryptionError<E>>
    for WgpsError<E, StoreCreationError, StoreError>
{
    fn from(value: EncryptionError<E>) -> Self {
        match value {
            EncryptionError::Inner(err) => WgpsError::Transport(err),
            EncryptionError::NoncesExhausted => WgpsError::NonceExhausted,
        }
    }
}

impl<E, StoreCreationError, StoreError> From<DecryptionError<E>>
    for WgpsError<E, StoreCreationError, StoreError>
{
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

impl<E, StoreCreationError, StoreError> From<HandshakeError<E, (), E>>
    for WgpsError<E, StoreCreationError, StoreError>
{
    fn from(value: HandshakeError<E, (), E>) -> Self {
        Self::Handshake(value)
    }
}

impl<E, StoreCreationError, StoreError> From<GlobalMessageError<(), DecryptionError<E>, Blame>>
    for WgpsError<E, StoreCreationError, StoreError>
{
    fn from(value: GlobalMessageError<(), DecryptionError<E>, Blame>) -> Self {
        match value {
            GlobalMessageError::Producer(err) => Self::from(err),
            GlobalMessageError::DecodeGlobalMessage(_)
            | GlobalMessageError::UnsupportedChannel(_)
            | GlobalMessageError::Other => Self::PeerMisbehaved,
        }
    }
}

impl<E, StoreCreationError, StoreError> From<AoiOutputError<EncryptionError<E>>>
    for WgpsError<E, StoreCreationError, StoreError>
{
    fn from(value: AoiOutputError<EncryptionError<E>>) -> Self {
        match value {
            AoiOutputError::Transport(err) => Self::from(err),
            AoiOutputError::NoEnumerationCapability => Self::NoEnumerationCapability,
            AoiOutputError::InvalidData | AoiOutputError::ReceivedAnnouncementError(_) | AoiOutputError::ReceivedBindCapabilityError(_) => Self::PeerMisbehaved,
            AoiOutputError::CapabilityChannelClosed => panic!("Hey, you triggered a bug! Here is what happened (please report this to the authors of the willow-rs crate): An `AoiOutputError::CapabilityChannelClosed` was attempted to be converted into a WgpsError, instead of handling it properly."),
        }
    }
}

impl<E, StoreCreationError, StoreError>
    From<RbsrError<EncryptionError<E>, StoreCreationError, StoreError>>
    for WgpsError<E, StoreCreationError, StoreError>
{
    fn from(value: RbsrError<EncryptionError<E>, StoreCreationError, StoreError>) -> Self {
        match value {
            RbsrError::Sending(err) => Self::from(err),
            RbsrError::StoreCreation(err) => WgpsError::StoreCreation(err),
            RbsrError::Store(err) => WgpsError::Store(err),
            RbsrError::ReconciliationChannelClosedByPeer =>panic!("Hey, you triggered a bug! Here is what happened (please report this to the authors of the willow-rs crate): An `RbsrError::ReconciliationChannelClosedByPeer` was attempted to be converted into a WgpsError, instead of handling it properly."),
            RbsrError::DataChannelClosedByPeer  => panic!("Hey, you triggered a bug! Here is what happened (please report this to the authors of the willow-rs crate): An `RbsrError::DataChannelClosedByPeer` was attempted to be converted into a WgpsError, instead of handling it properly."),
        }
    }
}

impl<
        E: core::fmt::Display,
        StoreCreationError: core::fmt::Display,
        StoreError: core::fmt::Display,
    > core::fmt::Display for WgpsError<E, StoreCreationError, StoreError>
{
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
            WgpsError::NoEnumerationCapability => {
                write!(f, "We were supposed to transmit the enumeration capability for a read capability of ours, but none was available.")
            }
            WgpsError::StoreCreation(err) => write!(f, "{}", err),
            WgpsError::Store(err) => write!(f, "{}", err),
            WgpsError::OurFault => {
                write!(
                    f,
                    "We received valid data, but could not process it due to limitations of our own (for example, running out of memory, or being unable to store a 64 bit integer in a usize)."
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOptions<
    const HANDSHAKE_HASHLEN_IN_BYTES: usize,
    const PIO_INTEREST_HASH_LENGTH_IN_BYTES: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    DH: DiffieHellmanSecretKey,
    N,
    S,
    PD,
    AT,
> {
    /* Handshake Options */
    pub is_initiator: bool,
    pub esk: DH,
    pub epk: DH::PublicKey,
    pub ssk: DH,
    pub spk: DH::PublicKey,
    pub protocol_name: &'static [u8],
    pub prologue: &'static [u8],
    /* End of Handshake Options */
    /* LCMUX channel options */
    pub reconciliation_channel_options: ChannelOptions,
    pub data_channel_options: ChannelOptions,
    pub overlap_channel_options: ChannelOptions,
    pub capability_channel_options: ChannelOptions,
    pub payload_request_channel_options: ChannelOptions,
    /// How many message bytes to receive at most on the overlap channel.
    pub overlap_channel_receiving_limit: u64,
    /// How many message bytes to receive at most on the capability channel.
    pub capability_channel_receiving_limit: u64,
    /// How many message bytes to receive at most on the payload_request channel.
    pub payload_request_channel_receiving_limit: u64,
    /* End of LCMUX Channel Options */
    /* Private Interest Overlap Options */
    hash_private_interest: fn(
        &PrivateInterest<MCL, MCC, MPL, N, S>,
        &[u8; HANDSHAKE_HASHLEN_IN_BYTES],
    ) -> [u8; PIO_INTEREST_HASH_LENGTH_IN_BYTES],
    /* End of Private Interest Overlap Options */
    /* RBSR options */
    /// The max number of entries to buffer in memory while receiving `ReconciliationSendEntry` messages and detecting duplicates to filter which entries to send in reply.
    entry_buffer_capacity: usize,
    session_id: u64,
    default_authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    partition_ops: PartitionOpts,
    /* End of RBSR Options */
}

pub fn sync_with_peer<
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
    PD,
    AT,
    MyReadCap,
    MyEnumCap,
    TheirReadCap,
    TheirEnumCap,
    Store,
    StoreCreationFunction,
    StoreCreationError,
    FP,
>(
    options: SyncOptions<
        HANDSHAKE_HASHLEN_IN_BYTES,
        PIO_INTEREST_HASH_LENGTH_IN_BYTES,
        MCL,
        MCC,
        MPL,
        DH,
        N,
        S,
        PD,
        AT,
    >,
    consumer: C,
    producer: P,
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
    chunk_to_byte_fun: fn(u64) -> u64,
    chunk_offset_and_count_to_byte_length: fn(u64 /* offset */, u64 /* count */) -> u64,
) -> (
    AoiInput<
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
    impl Future<Output = Result<(), WgpsError<E, StoreCreationError, Store::Error>>>,
)
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
    PD: WgpsPayloadDigest,
    AT: WgpsAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    MyReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    MyEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
    TheirReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    TheirEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
    StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError> + 'static,
    StoreCreationError: 'static,
    Store: RbsrStore<MCL, MCC, MPL, N, S, PD, AT, FP>,
    FP: WgpsFingerprint<MCL, MCC, MPL, N, S, PD, AT>,
{
    let stop_inputting_aois = Rc::new(Cell::new(false));
    let obtaining_the_aoi_consumer = Rc::new(TakeCell::new());

    let aoi_input = AoiInput {
        obtaining_the_actual_consumer: obtaining_the_aoi_consumer.clone(),
        the_actual_consumer: None,
        stop_accepting_aois: stop_inputting_aois.clone(),
    };

    return (
        aoi_input,
        do_sync_with_peer::<
            HANDSHAKE_HASHLEN_IN_BYTES,
            HANDSHAKE_BLOCKLEN_IN_BYTES,
            HANDSHAKE_PK_ENCODING_LENGTH_IN_BYTES,
            HANDSHAKE_TAG_WIDTH_IN_BYTES,
            HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_2,
            HANDSHAKE_TAG_WIDTH_IN_BYTES_PLUS_4096,
            HANDSHAKE_NONCE_WIDTH_IN_BYTES,
            HANDSHAKE_IS_TAG_PREPENDED,
            PIO_INTEREST_HASH_LENGTH_IN_BYTES,
            MCL,
            MCC,
            MPL,
            H,
            DH,
            AEAD,
            E,
            C,
            P,
            N,
            S,
            PD,
            AT,
            MyReadCap,
            MyEnumCap,
            TheirReadCap,
            TheirEnumCap,
            Store,
            StoreCreationFunction,
            StoreCreationError,
            FP,
        >(
            options,
            consumer,
            producer,
            storedinator,
            chunk_to_byte_fun,
            chunk_offset_and_count_to_byte_length,
            stop_inputting_aois,
            obtaining_the_aoi_consumer,
        ),
    );
}

async fn do_sync_with_peer<
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
    PD,
    AT,
    MyReadCap,
    MyEnumCap,
    TheirReadCap,
    TheirEnumCap,
    Store,
    StoreCreationFunction,
    StoreCreationError,
    FP,
>(
    options: SyncOptions<
        HANDSHAKE_HASHLEN_IN_BYTES,
        PIO_INTEREST_HASH_LENGTH_IN_BYTES,
        MCL,
        MCC,
        MPL,
        DH,
        N,
        S,
        PD,
        AT,
    >,
    consumer: C,
    producer: P,
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
    chunk_to_byte_fun: fn(u64) -> u64,
    chunk_offset_and_count_to_byte_length: fn(u64 /* offset */, u64 /* count */) -> u64,
    stop_inputting_aois: Rc<Cell<bool>>,
    obtaining_the_aoi_consumer: Rc<
        TakeCell<
            MyAoiInput<
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
) -> Result<(), WgpsError<E, StoreCreationError, Store::Error>>
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
    PD: WgpsPayloadDigest,
    AT: WgpsAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    MyReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    MyEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
    TheirReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    TheirEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
    StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError> + 'static,
    StoreCreationError: 'static,
    Store: RbsrStore<MCL, MCC, MPL, N, S, PD, AT, FP>,
    FP: WgpsFingerprint<MCL, MCC, MPL, N, S, PD, AT>,
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

    let my_salt = if options.is_initiator {
        salt_base
    } else {
        invert_bytes(salt_base)
    };

    //////////////////
    // LCMUX Things //
    //////////////////

    let shared_consumer_state = shared_consumer::State::new(c);
    let consumer = SharedConsumer::new(Rc::new(shared_consumer_state));

    let shared_producer_state = shared_producer::State::new(p);
    let producer = SharedProducer::new(Rc::new(shared_producer_state));

    let mut lcmux_session: Session<
        5,
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
            &options.payload_request_channel_options,
        ],
        [false, false, true, true, true],
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
            InitOptions {
                receiving_limit: Some(options.payload_request_channel_receiving_limit),
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

    let [reconciliation_channel_sender, data_channel_sender, overlap_channel_sender, capability_channel_sender, payload_request_channel_sender] =
        channel_senders;
    let [mut reconciliation_channel_receiver, mut data_channel_receiver, overlap_channel_receiver, capability_channel_receiver, payload_request_channel_receiver] =
        channel_receivers;

    let data_channel_sender = Rc::new(Mutex::new(data_channel_sender));

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
        mut overlap_output,
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

    *global_receiver.get_relative_to_mut() = Some(pio_state.clone());

    // Move `my_aoi_input` into the actual aoi input.
    obtaining_the_aoi_consumer.set(my_aoi_input);

    /////////////////
    // RBSR Things //
    /////////////////

    let reconciliation_sender = Rc::new(ReconciliationSender::<
        HANDSHAKE_HASHLEN_IN_BYTES,
        PIO_INTEREST_HASH_LENGTH_IN_BYTES,
        MCL,
        MCC,
        MPL,
        N,
        S,
        PD,
        AT,
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
        Store,
        StoreCreationFunction,
        FP,
    >::new(
        pio_state.clone(),
        storedinator.clone(),
        reconciliation_channel_sender,
        data_channel_sender.clone(),
        options.session_id,
        options.default_authorised_entry.clone(),
    ));

    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    // Every unit of work that the WGPS needs to perform is defined as a future in what follows, via an async block. //
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    // A future that powers the lcmux connection.
    let do_the_lcmux_bookkeeping = async {
        bookkeeping
            .keep_the_books()
            .await
            .map_err::<WgpsError<E, StoreCreationError, Store::Error>, _>(|err| match err {
                Some(EncryptionError::Inner(channel_err)) => WgpsError::from(channel_err),
                Some(EncryptionError::NoncesExhausted) => WgpsError::NonceExhausted,
                None => WgpsError::PeerMisbehaved,
            })
    };

    // A future to receive global messages and act on them.
    let receive_global_messages = async {
        loop {
            match global_receiver.produce().await? {
                Left(GlobalMessage::PioAnnounceOverlap(msg)) => {
                    // We have an in-memory-channel into which to enqueue all PioAnnounceOverlap messages we receive.
                    // The PIO implementation then does the remainder of the work.
                    announce_overlap_in_memory_sender.consume(msg).await.unwrap(/* infallible */);
                }
                Left(GlobalMessage::DataSetEagerness(_))
                | Left(GlobalMessage::ResourceHandleFree(_)) => {
                    // Ours is a selfish and primitive implementation, we ignore both of these right now (which the spec explicitly allows, since both messages merely serve for optimisation purposes).

                    // Do nothing, simply continue to the next iteration of the loop.
                }
                Right(()) => {
                    // We won't receive more global messages from the other peer. That does not necessarily terminate reconciliation, but we *can* stop polling for *more* global messages.
                    return Ok(());
                }
            }
        }
    };

    // Stores information per intersection between the AoIs the two peers want to sync.
    let intersection_info = Rc::new(Mutex::new(HashMap::<
        IntersectionKey,
        IntersectionInfo<MCL, MCC, MPL, N, S>,
    >::new()));

    // As part of synchronising data, we obtain store subscriptions. We want to process all of these subscriptions concurrently, and forward new entries to the peer. Doing so is a bit tricky, we basically go with approach sketched here:
    // https://rust-lang.github.io/wg-async/vision/submitted_stories/status_quo/barbara_battles_buffered_streams.html#is-there-any-way-for-barbara-to-both-produce-and-process-work-items-simultaneously-without-the-buffering-and-so-forth
    //
    // For this approach, we need a stream of all subscriptions we want to process. But subscriptions can occur in three different futures. Hence, we collect them in a mpsc channel.
    let (
        mut subscriptions_we_want_to_process_sender,
        mut subscriptions_we_want_to_process_receiver12,
    ) = futures::channel::mpsc::unbounded();
    let mut subscriptions_we_want_to_process_sender2 =
        subscriptions_we_want_to_process_sender.clone();
    let (
        mut subscriptions_we_want_to_process_sender3,
        mut subscriptions_we_want_to_process_receiver3,
    ) = futures::channel::mpsc::unbounded();

    // A future to listen for overlaps detected in pio and to initiate set reconciliation if appropriate.
    let initiate_rbsr_storedinator = storedinator.clone();
    let initiate_rbsr = async {
        if options.is_initiator {
            loop {
                match overlap_output.produce().await {
                    Ok(Right(())) => return Ok(()), // nothing special needs to happen when pio terminates
                    Ok(Left(common_areas)) => {
                        for common_area in common_areas.into_iter() {
                            intersection_info.write().await.insert(
                                IntersectionKey {
                                    my_handle: common_area.my_handle,
                                    their_handle: common_area.their_handle,
                                },
                                IntersectionInfo {
                                    namespace_id: common_area.namespace.clone(),
                                    aoi: common_area.aoi.clone(),
                                    max_payload_power: common_area.max_payload_power,
                                },
                            );

                            let store = initiate_rbsr_storedinator
                                .get_store(&common_area.namespace)
                                .await
                                .map_err(|store_creation_error| {
                                    RbsrError::StoreCreation(store_creation_error)
                                })?;

                            let range = store.area_of_interest_to_range(&common_area.aoi).await;

                            let (fp, count) = store
                                .summarise(&range)
                                .await
                                .map_err(|store_error| RbsrError::Store(store_error))?;

                            match reconciliation_sender
                                .process_split_action(
                                    &common_area.namespace,
                                    &range,
                                    if count < options.partition_ops.min_range_size {
                                        SplitAction::SendEntries(count)
                                    } else {
                                        SplitAction::SendFingerprint(fp)
                                    },
                                    0,
                                    common_area.my_handle,
                                    common_area.their_handle,
                                )
                                .await?
                            {
                                None => {
                                    // nothing else to do
                                }
                                Some(subscription) => {
                                    // Store the subscription for later processing. In case of an error, the subscription will not be processed later, but we can ignore that here.
                                    let _ = subscriptions_we_want_to_process_sender
                                        .send(subscription)
                                        .await;
                                }
                            }
                        }
                    }
                    Err(AoiOutputError::CapabilityChannelClosed) => {
                        // This error is non-fatal. We must not attempt inputting more Aois, but the remainder of the sync session can continue running without problems.
                        stop_inputting_aois.set(true);
                    }
                    Err(fatal_err) => return Err(WgpsError::from(fatal_err)),
                }
            }
        } else {
            return Ok(());
        }
    };

    // A future to process all incoming messages on the reconciliation channel.
    let process_incoming_fingerprint_storedinator = storedinator.clone();
    let process_incoming_fingerprint_messages = async {
        let mut previously_received_fingerprint_3drange: Range3d<MCL, MCC, MPL, S> =
            Range3d::default();

        let mut their_next_root_id = if options.is_initiator { 2 } else { 1 };

        loop {
            match ReconciliationSendFingerprint::<MCL, MCC, MPL, S, FP>::relative_decode(
                &mut reconciliation_channel_receiver,
                &previously_received_fingerprint_3drange,
            )
            .await
            {
                Err(DecodeError::UnexpectedEndOfInput(())) => {
                    // They closed their reconciliation channel, stop processing it.
                    return Ok(());
                }
                Err(DecodeError::Producer(())) | Err(DecodeError::Other(Blame::TheirFault)) => {
                    return Err(WgpsError::PeerMisbehaved)
                }
                Err(DecodeError::Other(Blame::OurFault)) => return Err(WgpsError::OurFault),
                Ok(fp_msg) => {
                    let namespace_id = match intersection_info.read().await.get(&IntersectionKey {
                        my_handle: fp_msg.info.receiver_handle,
                        their_handle: fp_msg.info.sender_handle,
                    }) {
                        None => return Err(WgpsError::PeerMisbehaved),
                        Some(intersection_data) => intersection_data.namespace_id.clone(),
                    };

                    let root_id = if fp_msg.info.root_id != 0 {
                        fp_msg.info.root_id
                    } else {
                        their_next_root_id += 1;
                        their_next_root_id - 1
                    };

                    let store = process_incoming_fingerprint_storedinator
                        .get_store(&namespace_id)
                        .await
                        .map_err(|store_creation_error| {
                            RbsrError::StoreCreation(store_creation_error)
                        })?;

                    let (my_fp, _my_count) = store
                        .summarise(&fp_msg.info.range)
                        .await
                        .map_err(|store_error| RbsrError::Store(store_error))?;

                    if my_fp == fp_msg.fingerprint {
                        // Nothing more to do with this message.
                    } else {
                        let mut partitions = store
                            .partition_range(fp_msg.info.range.clone(), options.partition_ops)
                            .await
                            .map_err(|store_error| RbsrError::Store(store_error))?;

                        loop {
                            match partitions
                                .produce()
                                .await
                                .map_err(|store_error| RbsrError::Store(store_error))?
                            {
                                Left((range, action)) => {
                                    match reconciliation_sender.process_split_action(&namespace_id, &range, action, root_id, fp_msg.info.receiver_handle, fp_msg.info.sender_handle /* we swap ereceiver and sender handle compared to what we received */).await? {
                                None => {
                                    // nothing else to do
                                }
                                Some(subscription) => {
                                    // Store the subscription for later processing. In case of an error, the subscription will not be processed later, but we can ignore that here.
                                    let _ = subscriptions_we_want_to_process_sender2
                                        .send(subscription)
                                        .await;
                                }
                            }
                                }
                                Right(()) => break,
                            }
                        }
                    }

                    previously_received_fingerprint_3drange = fp_msg.info.range.clone();
                }
            }
        }
    };

    // A future to receive data-channel messages and act on them.
    let receive_data_messages = async {
        let r = DecodeDataMessagesRelativeToThis::new(options.default_authorised_entry.clone());

        // Buffers the received entries after a ReconciliationAnnounceEntries message, so that our reply can omit them.
        let mut received_entry_buffer: Vec<LengthyEntry<MCL, MCC, MPL, N, S, PD>> =
            Vec::with_capacity(options.entry_buffer_capacity);

        // State per announced range.
        let mut received_all_messages_for_current_range = false;
        let mut they_want_response_for_current_range = false;
        let mut root_id_for_current_range = 0;
        let mut my_handle_for_current_range = 0;
        let mut their_handle_for_current_range = 0;

        // State per rbsr-transmitted entry (and its payload).
        // This flag ensures that receiving the payload for the same entry concurrently form multiple peers doesn't result in storing an incorrect payload.
        let mut try_appending_payload = false;
        let mut payload_current_chunk_offset = 0;

        // State per data-transmitted entry (and its payload).
        let mut data_payload_current_chunk_offset = 0;

        loop {
            match DataMessage::<MCL, MCC, MPL, N, S, PD, AT>::relative_decode(
                &mut data_channel_receiver,
                &r,
            )
            .await
            {
                Err(DecodeError::UnexpectedEndOfInput(())) => {
                    // They closed their data channel, stop processing it.
                    return Ok(());
                }
                Err(DecodeError::Producer(())) | Err(DecodeError::Other(Blame::TheirFault)) => {
                    return Err(WgpsError::PeerMisbehaved)
                }
                Err(DecodeError::Other(Blame::OurFault)) => return Err(WgpsError::OurFault),
                Ok(DataMessage::ReconciliationAnnounceEntries(announce_msg)) => {
                    *r.previously_received_itemset_3drange.write().await =
                        announce_msg.info.range.clone();

                    *r.granted_namespace_of_info_of_preceding_announce_entries_message
                        .write()
                        .await = match intersection_info.read().await.get(&IntersectionKey {
                        my_handle: announce_msg.info.receiver_handle,
                        their_handle: announce_msg.info.sender_handle,
                    }) {
                        None => return Err(WgpsError::PeerMisbehaved),
                        Some(intersection_data) => intersection_data.namespace_id.clone(),
                    };

                    received_entry_buffer.clear();
                    received_all_messages_for_current_range = announce_msg.is_empty;
                    they_want_response_for_current_range = announce_msg.want_response;
                    root_id_for_current_range = announce_msg.info.root_id;
                    my_handle_for_current_range = announce_msg.info.receiver_handle;
                    their_handle_for_current_range = announce_msg.info.sender_handle;

                    // That's it. Leave the match statement and continue after it, which means sending a response if received_all_messages_for_current_range.
                }

                Ok(DataMessage::ReconciliationSendEntry(entry_msg)) => {
                    *r.reconciliation_current_entry.write().await = entry_msg.entry.entry().clone();

                    payload_current_chunk_offset = entry_msg.offset;

                    let store = storedinator
                        .get_store(entry_msg.entry.entry().entry().namespace_id())
                        .await
                        .map_err(|store_creation_error| {
                            RbsrError::StoreCreation(store_creation_error)
                        })?;

                    match store
                        .ingest_entry(
                            entry_msg.entry.entry().clone(),
                            false,
                            EntryOrigin::Remote(options.session_id),
                        )
                        .await {
                            Ok(_) => {/* no-op */}
                            Err(EntryIngestionError::PruningPrevented) => panic!("Got a EntryIngestionError::PruningPrevented error despite calling Store::ingest_entry with prevent_pruning set to false. The store implementation is buggy."),
                            Err(EntryIngestionError::OperationsError(store_err)) => return Err(WgpsError::Store(store_err)),
                        }

                    if received_entry_buffer.len() < options.entry_buffer_capacity {
                        received_entry_buffer.push(entry_msg.entry.clone().into_lengthy_entry());
                    }

                    try_appending_payload = true;
                }

                Ok(DataMessage::ReconciliationSendPayload(payload_msg)) => {
                    let payload_start_byte_offset = chunk_to_byte_fun(payload_current_chunk_offset);
                    let transmitted_payload_bytes_in_this_message =
                        chunk_offset_and_count_to_byte_length(
                            payload_current_chunk_offset,
                            payload_msg.amount,
                        );

                    if try_appending_payload {
                        let entry = r.reconciliation_current_entry.read().await.entry().clone();

                        let store = storedinator.get_store(entry.namespace_id()).await.map_err(
                            |store_creation_error| RbsrError::StoreCreation(store_creation_error),
                        )?;

                        let _ = data_channel_receiver.produce().await;

                        let mut bytes_to_append = ufotofu::producer::Limit::new(
                            BorrowedProducer(&mut data_channel_receiver),
                            transmitted_payload_bytes_in_this_message as usize,
                        );

                        match store
                            .append_payload(
                                entry.subspace_id(),
                                entry.path(),
                                Some(entry.payload_digest().clone()),
                                Some(payload_start_byte_offset),
                                &mut bytes_to_append,
                            )
                            .await
                        {
                            Ok(_) => { /* no-op */ }
                            Err(PayloadAppendError::OperationError(store_err)) => {
                                return Err(WgpsError::Store(store_err));
                            }
                            Err(_) => {
                                try_appending_payload = false;
                            }
                        }
                    }

                    payload_current_chunk_offset += payload_msg.amount;
                }

                Ok(DataMessage::ReconciliationTerminatePayload(terminate_payload_msg)) => {
                    received_all_messages_for_current_range = terminate_payload_msg.is_final;

                    // That's it. Leave the match statement and continue after it, which means sending a response if received_all_messages_for_current_range.
                }

                Ok(DataMessage::DataSendEntry(entry_msg)) => {
                    *r.data_current_entry.write().await = entry_msg.entry.clone();

                    data_payload_current_chunk_offset = entry_msg.offset;

                    let store = storedinator
                        .get_store(entry_msg.entry.entry().namespace_id())
                        .await
                        .map_err(|store_creation_error| {
                            RbsrError::StoreCreation(store_creation_error)
                        })?;

                    match store
                        .ingest_entry(
                            entry_msg.entry.clone(),
                            false,
                            EntryOrigin::Remote(options.session_id),
                        )
                        .await {
                            Ok(_) => {/* no-op */}
                            Err(EntryIngestionError::PruningPrevented) => panic!("Got a EntryIngestionError::PruningPrevented error despite calling Store::ingest_entry with prevent_pruning set to false. The store implementation is buggy."),
                            Err(EntryIngestionError::OperationsError(store_err)) => return Err(WgpsError::Store(store_err)),
                        }

                    // Explicitly continue to the next iteration of the loop; the logic after the loop must only be executed when receiving rbsr messages.
                    continue;
                }

                Ok(DataMessage::DataSendPayload(payload_msg)) => {
                    let payload_start_byte_offset =
                        chunk_to_byte_fun(data_payload_current_chunk_offset);
                    let transmitted_payload_bytes_in_this_message =
                        chunk_offset_and_count_to_byte_length(
                            data_payload_current_chunk_offset,
                            payload_msg.amount,
                        );

                    let entry = r.reconciliation_current_entry.read().await.entry().clone();

                    let store = storedinator.get_store(entry.namespace_id()).await.map_err(
                        |store_creation_error| RbsrError::StoreCreation(store_creation_error),
                    )?;

                    let _ = data_channel_receiver.produce().await;

                    let mut bytes_to_append = ufotofu::producer::Limit::new(
                        BorrowedProducer(&mut data_channel_receiver),
                        transmitted_payload_bytes_in_this_message as usize,
                    );

                    match store
                        .append_payload(
                            entry.subspace_id(),
                            entry.path(),
                            Some(entry.payload_digest().clone()),
                            Some(payload_start_byte_offset),
                            &mut bytes_to_append,
                        )
                        .await
                    {
                        Ok(_) => { /* no-op */ }
                        Err(PayloadAppendError::OperationError(store_err)) => {
                            return Err(WgpsError::Store(store_err));
                        }
                        Err(_) => {
                            try_appending_payload = false;
                        }
                    }

                    data_payload_current_chunk_offset += payload_msg.amount;

                    // Explicitly continue to the next iteration of the loop; the logic after the loop must only be executed when receiving rbsr messages.
                    continue;
                }

                Ok(_) => todo!("Handle other, non-reconciliation messages"),
                // DataSendEntry(DataSendEntry<MCL, MCC, MPL, N, S, PD, AT>),
                // DataSendPayload(DataSendPayload),
                // PayloadRequestSendResponse(PayloadRequestSendResponse),
            }

            // The following code is only reached after processing a reconciliation message (all other messages `continue` to the next iteration of the data-message-decoding-loop inside the above match statement)
            if received_all_messages_for_current_range && they_want_response_for_current_range {
                // Sort buffered received entries so we can query more efficiently whether a given entry is currently buffered.
                received_entry_buffer.sort_by(order_entries);

                let subscription = reconciliation_sender
                    .send_entries_in_range(
                        &*r.granted_namespace_of_info_of_preceding_announce_entries_message
                            .read()
                            .await,
                        &*r.previously_received_itemset_3drange.read().await,
                        root_id_for_current_range,
                        my_handle_for_current_range,
                        their_handle_for_current_range,
                        Some(&received_entry_buffer[..]),
                    )
                    .await?;

                // Store the subscription for later processing. In case of an error, the subscription will not be processed later, but we can ignore that here.
                let _ = subscriptions_we_want_to_process_sender3
                    .send(subscription)
                    .await;
            }

            // All done processing the current reconciliation message; move on to the beginning of the loop, decoding the next data message.
        }
    };

    // An extra clone that can be moved into the `forward_new_entries` future.
    let data_channel_sender_clone = data_channel_sender.clone();
    // A future to collect store subscriptions and forward new entries to the peer.
    let forward_new_entries = async {
        let mut push_new_entries: futures::stream::FuturesUnordered<
            Pin<Box<dyn futures::Future<Output = Result<_, RbsrError<_, StoreCreationError, _>>>>>,
        > = futures::stream::FuturesUnordered::new();

        let data_current_entry = Rc::new(Mutex::new(options.default_authorised_entry.clone()));

        loop {
            futures::select! {
                new_subscription_to_add = subscriptions_we_want_to_process_receiver12.next() => {
                    match new_subscription_to_add {
                        None => {
                            // do nothing, the FuturesUnordered will eventually be done if it isn't refilled
                        }
                        Some(new_subscription) => push_new_entries.push(process_subscription(new_subscription.0, new_subscription.1, data_channel_sender_clone.clone(), data_current_entry.clone(), options.session_id)),
                    }
                }
                new_subscription_to_add = subscriptions_we_want_to_process_receiver3.next() => {
                    match new_subscription_to_add {
                        None => {
                            // do nothing, the FuturesUnordered will eventually be done if it isn't refilled
                        }
                        Some(new_subscription) => push_new_entries.push(process_subscription(new_subscription.0, new_subscription.1, data_channel_sender_clone.clone(), data_current_entry.clone(), options.session_id)),
                    }
                }
                _done_with_a_subscription = push_new_entries.next() => {
                    // nothing to do here. We just keep polling those futures, each of which processes a subscription
                }
            }
        }

        // This is unreachable, but it allows the compiler to infer the correct return type of this future so that the `try_join!` below compiles.
        #[allow(unreachable_code)]
        return Result::<(), WgpsError<_, _, _>>::Ok(());
    };

    // Actually run all the concurrent operations of the WGPS.
    let _ = try_join!(
        do_the_lcmux_bookkeeping,
        receive_global_messages,
        initiate_rbsr,
        process_incoming_fingerprint_messages,
        receive_data_messages,
        forward_new_entries,
    )?;

    Ok(())
}

fn invert_bytes<const LEN: usize>(mut bytes: [u8; LEN]) -> [u8; LEN] {
    for byte in bytes.iter_mut() {
        *byte ^= 255;
    }

    bytes
}

/// A consumer of the [`CapableAoi`]s you want to sync.
pub struct AoiInput<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    MyReadCap,
    MyEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
> {
    obtaining_the_actual_consumer: Rc<
        TakeCell<
            MyAoiInput<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                MyReadCap,
                MyEnumCap,
                P,
                PFinal,
                PErr,
                C,
                CErr,
            >,
        >,
    >,
    the_actual_consumer: Option<
        MyAoiInput<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            MyReadCap,
            MyEnumCap,
            P,
            PFinal,
            PErr,
            C,
            CErr,
        >,
    >,
    stop_accepting_aois: Rc<Cell<bool>>,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    > Consumer
    for AoiInput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    MyReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Eq
        + Hash
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
    MyEnumCap: Eq
        + Hash
        + Clone
        + EnumerationCapability<Receiver = S, NamespaceId = N>
        + RelativeEncodableKnownSize<(N, S)>,
    C: Consumer<Item = u8, Final = (), Error = CErr> + BulkConsumer,
    CErr: Clone,
{
    type Item = CapableAoi<MCL, MCC, MPL, MyReadCap, MyEnumCap>;

    type Final = ();

    type Error = WgpsInputError;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        loop {
            match self.the_actual_consumer.as_mut() {
                None => {
                    self.the_actual_consumer =
                        Some(self.obtaining_the_actual_consumer.take().await);
                    // Go to next loop iteration, where the actual consumer now is available.
                }
                Some(actual_consumer) => {
                    if self.stop_accepting_aois.get() {
                        return Err(WgpsInputError::Done);
                    } else {
                        return Ok(actual_consumer.consume(item).await?);
                    }
                }
            }
        }
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        loop {
            match self.the_actual_consumer.as_mut() {
                None => {
                    self.the_actual_consumer =
                        Some(self.obtaining_the_actual_consumer.take().await);
                    // Go to next loop iteration, where the actual consumer now is available.
                }
                Some(actual_consumer) => {
                    return Ok(actual_consumer.close(fin).await?);
                }
            }
        }
    }
}

/// Everything that can go wrong when submitting an AreaOfInterest to the private interest overlap detection process.
pub enum WgpsInputError {
    /// We had to supply an enumeration capability, but the submitted CapableAoi didn't supply one.
    NoEnumerationCapability,
    /// Syncing with the peer had to be aborted for reasons outside our control.
    Fatal,
    /// The peer will not accept more input. Syncing will still continue for all areas consumed so far.
    Done,
}

impl<TransportError> From<AoiInputError<TransportError>> for WgpsInputError {
    fn from(value: AoiInputError<TransportError>) -> WgpsInputError {
        match value {
            AoiInputError::Transport(_) => {
                return WgpsInputError::Fatal;
            }
            AoiInputError::NoEnumerationCapability => {
                return WgpsInputError::NoEnumerationCapability;
            }
            AoiInputError::OverlapChannelClosed | AoiInputError::CapabilityChannelClosed => {
                return WgpsInputError::Done;
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct IntersectionKey {
    my_handle: u64,
    their_handle: u64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct IntersectionInfo<const MCL: usize, const MCC: usize, const MPL: usize, N, S> {
    namespace_id: N,
    aoi: AreaOfInterest<MCL, MCC, MPL, S>,
    max_payload_power: u8,
}

/// Options to specify how ranges should be partitioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PartitionOpts {
    /// The largest number of entries that can be included by a range before it is better to send that range's fingerprint instead of sending its entries.
    pub min_range_size: usize,
    /// The targeted number of partitions to split a range into. Must be at least 2.
    pub target_split_count: usize,
}

/// A split range and the action which should be taken with that split range during range-based set reconciliation.
pub type RangeSplit<const MCL: usize, const MCC: usize, const MPL: usize, S, FP> =
    (Range3d<MCL, MCC, MPL, S>, SplitAction<FP>);

/// Whether to send a split range's fingerprint or its included entries.
#[derive(Debug, Clone)]
pub enum SplitAction<FP> {
    SendFingerprint(FP),
    /// Stores an approximate number of entries in the range.
    SendEntries(usize),
}

/// A [`Store`] capable of performing [3d range-based set reconciliation](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3_range_based_set_reconciliation).
pub trait RbsrStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, FP>
where
    Self: Store<MCL, MCC, MPL, N, S, PD, AT> + 'static,
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
        self: Rc<Self>,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Subscribe to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Range3d`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn subscribe_range(
        self: Rc<Self>,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;

    fn query_and_subscribe_range(
        self: Rc<Self>,
        range: Range3d<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> Result<
        impl Producer<
            Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
            Final = impl Producer<
                Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
                Final = (),
                Error = Self::Error,
            >,
            Error = Self::Error,
        >,
        Self::Error,
    >;

    /// Summarise a [`Range3d`] as a [fingerprint](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3rbsr_fp).
    fn summarise(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
    ) -> impl Future<Output = Result<(FP, usize), Self::Error>>;

    /// Convert an [`AreaOfInterest`] to a concrete [`Range3d`] including all the entries the given [`AreaOfInterest`] would.
    fn area_of_interest_to_range(
        &self,
        aoi: &AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> impl Future<Output = Range3d<MCL, MCC, MPL, S>>;

    /// Partition a [`Range3d`] into many parts, or return `None` if the given range cannot be split (for instance because the range only includes a single entry).
    fn partition_range(
        self: Rc<Self>,
        range: Range3d<MCL, MCC, MPL, S>,
        options: PartitionOpts,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = RangeSplit<MCL, MCC, MPL, S, FP>, Final = (), Error = Self::Error>,
            Self::Error,
        >,
    >;
}

// TODO: Think through whether this could make sense in ufotofu proper, or even whether `&mut P` should implement `Producer` when `P` does.
struct BorrowedProducer<'p, P>(&'p mut P);

impl<'p, P> Producer for BorrowedProducer<'p, P>
where
    P: Producer,
{
    type Item = P::Item;
    type Final = P::Final;
    type Error = P::Error;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        self.0.produce().await
    }
}

impl<'p, P> BufferedProducer for BorrowedProducer<'p, P>
where
    P: BufferedProducer,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        self.0.slurp().await
    }
}

impl<'p, P> BulkProducer for BorrowedProducer<'p, P>
where
    P: BulkProducer,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        self.0.expose_items().await
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.0.consider_produced(amount).await
    }
}
