use std::{cell::Cell, future::Future, hash::Hash, rc::Rc};

use either::Either::{Left, Right};
use futures::try_join;
use lcmux::{ChannelOptions, GlobalMessageError, InitOptions, Session};
use ufotofu::{BulkConsumer, BulkProducer, Consumer, Producer};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    spsc, Mutex, OnceCell, TakeCell,
};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    Store, StoreEvent, SubspaceId,
};

mod parameters;
mod rbsr;
mod storedinator;

pub mod messages;
use messages::*;

use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, EncodableKnownSize, EncodableSync,
    RelativeEncodableKnownSize,
};
use willow_pio::{PersonalPrivateInterest, PrivateInterest};
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
    pio::{AoiInputError, AoiOutputError, CapableAoi, MyAoiInput, PioSession},
    rbsr::{RbsrError, ReconciliationSession},
    storedinator::Storedinator,
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
    /// We were supposed to transmit the enumeration capability for a read capability of ours, but none was available.
    NoEnumerationCapability,
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

impl<E> From<AoiOutputError<EncryptionError<E>>> for WgpsError<E> {
    fn from(value: AoiOutputError<EncryptionError<E>>) -> Self {
        match value {
            AoiOutputError::Transport(err) => Self::from(err),
            AoiOutputError::NoEnumerationCapability => Self::NoEnumerationCapability,
            AoiOutputError::InvalidData | AoiOutputError::ReceivedAnnouncementError(_) | AoiOutputError::ReceivedBindCapabilityError(_) => Self::PeerMisbehaved,
            AoiOutputError::CapabilityChannelClosed => panic!("Hey, you triggered a bug! Here is what happened (please report this to the authors of the willow-rs crate): An `AoiOutputError::CapabilityChannelClosed` was attempted to be converted into a WgpsError, instead of handling it properly."),
        }
    }
}

impl<E> From<RbsrError<EncryptionError<E>>> for WgpsError<E> {
    fn from(value: RbsrError<EncryptionError<E>>) -> Self {
        match value {
            RbsrError::Sending(err) => Self::from(err),
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
            WgpsError::NoEnumerationCapability => {
                write!(f, "We were supposed to transmit the enumeration capability for a read capability of ours, but none was available.")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /* RBSR options */
    /// The max number of entries to buffer in memory while receiving `ReconciliationSendEntry` messages and detecting duplicates to filter which entries to send in reply.
    entry_buffer_capacity: usize,
    /* End of RBSR Options */
}

pub fn sync_with_peer<
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
    Store,
    StoreCreationFunction,
>(
    options: SyncOptions<
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
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
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
    impl Future<Output = Result<(), WgpsError<E>>>
        + use<
            'prologue,
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
            MyReadCap,
            MyEnumCap,
            TheirReadCap,
            TheirEnumCap,
            Store,
            StoreCreationFunction,
        >,
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
    MyReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    MyEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
    TheirReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    TheirEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
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
            MyReadCap,
            MyEnumCap,
            TheirReadCap,
            TheirEnumCap,
            Store,
            StoreCreationFunction,
        >(
            options,
            consumer,
            producer,
            storedinator,
            stop_inputting_aois,
            obtaining_the_aoi_consumer,
        ),
    );
}

async fn do_sync_with_peer<
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
    Store,
    StoreCreationFunction,
>(
    options: SyncOptions<
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
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
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
    MyReadCap: WgpsReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    MyEnumCap: WgpsEnumerationCapability<NamespaceId = N, Receiver = S>,
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

    let ReconciliationSession {
        initiator: mut rbsr_initiator,
        receiver: mut rbsr_message_receiver,
    } = ReconciliationSession::new(
        pio_state.clone(),
        storedinator,
        reconciliation_channel_sender,
        reconciliation_channel_receiver,
        options.entry_buffer_capacity,
    );

    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    // Every unit of work that the WGPS needs to perform is defined as a future in what follows, via an async block. //
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    // A future that powers the lcmux connection.
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

    // A future to listen for overlaps detected in pio and to initiate set reconciliation if appropriate.
    let initiate_rbsr = async {
        if options.is_initiator {
            loop {
                match overlap_output.produce().await {
                    Ok(Right(())) => return Ok(()), // nothing special needs to happen when pio terminates
                    Ok(Left(common_areas)) => {
                        for common_area in common_areas.into_iter() {
                            rbsr_initiator.initiate_reconciliation(common_area).await?;
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

    // A future to process all incoming rbsr pessages.
    let process_incoming_rbsr_messages = async {
        rbsr_message_receiver
            .process_incoming_reconciliation_messages()
            .await?;
        Ok(())
    };

    // Actually run all the concurrent operations of the WGPS.
    let _ = try_join!(
        do_the_lcmux_bookkeeping,
        receive_global_messages,
        initiate_rbsr,
        process_incoming_rbsr_messages,
    )?; // TODO add futures for the remaining, as of yet unimplemented tasks: doing post-reconciliation forwarding, receiving `data` messages. Will be fleshed out as we progress with the WGPS implementation.

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

/// Options to specify how ranges should be partitioned.
#[derive(Debug, Clone, Copy)]
pub struct PartitionOpts {
    /// The largest number of entries that can be included by a range before it is better to send that range's fingerprint instead of sending its entries.
    pub min_range_size: usize,
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
    // TODO this should be a usize, and marked as approximate!
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

    fn query_and_subscribe_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = Result<
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
        >,
    >;

    /// Summarise a [`Range3d`] as a [fingerprint](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3rbsr_fp).
    fn summarise(&self, range: Range3d<MCL, MCC, MPL, S>) -> impl Future<Output = (FP, u64)>;

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
