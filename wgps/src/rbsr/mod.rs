use std::marker::PhantomData;
use std::rc::Rc;

use either::Either::{self, Left, Right};
use lcmux::{ChannelReceiver, ChannelSender};
use ufotofu::producer::FromSlice;
use wb_async_utils::Mutex;
use willow_data_model::{
    Component, EntryIngestionError, EntryOrigin, LengthyAuthorisedEntry, Path, PathBuilder,
    PayloadAppendError,
};

use crate::messages::{
    ReconciliationAnnounceEntries, ReconciliationSendEntry, ReconciliationSendFingerprint,
    ReconciliationSendPayload, ReconciliationTerminatePayload,
};
use crate::pio::overlap_finder::NamespacedAoIWithMaxPayloadPower;
use crate::{pio, storedinator::Storedinator};

/// The state per WGPS session for handling RBSR.
struct ReconciliationState<
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
    Store,
    StoreCreationFunction,
    Fingerprint,
    PD,
    AT,
> {
    /// Need this to decode messages and to check validity of incoming messages (whether their capability-handle pairs are valid and refer to overlapping areas).
    pio_state: Rc<
        pio::State<
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
    /// Access to our stores.
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
    reconciliation_channel_sender: Mutex<ChannelSender<4, P, PFinal, PErr, C, CErr>>,
    session_id: u64,

    // temp
    todoRemoveThis: PhantomData<(Fingerprint, PD, AT)>,
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
    ReconciliationState<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
{
    fn new(
        pio_state: Rc<
            pio::State<
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
        storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
        reconciliation_channel_sender: Mutex<ChannelSender<4, P, PFinal, PErr, C, CErr>>,
        session_id: u64,
    ) -> Self {
        Self {
            pio_state,
            storedinator,
            reconciliation_channel_sender,
            session_id,
            todoRemoveThis: PhantomData,
        }
    }
}

/// The entrypoint to reconciliatoin (see the associated `new` function).
pub(crate) struct ReconciliationSession<
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
    Store,
    StoreCreationFunction,
    Fingerprint,
    PD,
    AT,
> {
    /// Use this value to initiate reconciliation for detected overlaps (only if you initiated the reconciliation session, otherwise ignore overlaps).
    pub initiator: ReconciliationInitiator<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >,
    /// Use this value to process all incoming messages on the reconciliation channel (via the `process_incoming_reconciliation_messages` method).
    pub receiver: ReconciliationReceiver<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >,
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
    ReconciliationSession<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
{
    /// The entrypoint to reconciliation. Use the public fields of this struct for the separate sub-task.
    ///
    /// `entry_buffer_capacity` is the max number of entries to buffer in memory while receiving `ReconciliationSendEntry` messages and detecting duplicates to filter which entries to send in reply.
    ///
    /// `session_id` is an arbitrary numeric identifier that must differ from the id of all other reconciliation sessions. It is used internally to ensure that we do not echo the entries we receive from a peer back to them.
    pub fn new(
        pio_state: Rc<
            pio::State<
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
        storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
        reconciliation_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
        reconciliation_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
        entry_buffer_capacity: usize,
        session_id: u64,
    ) -> Self {
        let state = Rc::new(ReconciliationState::new(
            pio_state,
            storedinator,
            Mutex::new(reconciliation_channel_sender),
            session_id,
        ));

        Self {
            initiator: ReconciliationInitiator {
                state: state.clone(),
            },
            receiver: ReconciliationReceiver {
                state: state,
                receiver: reconciliation_channel_receiver,
                phantom: PhantomData,
                received_entry_buffer: Vec::with_capacity(entry_buffer_capacity),
                received_entry_buffer_capacity: entry_buffer_capacity,
            },
        }
    }
}

/// Use this struct to initiate reconciliation for detected overlaps (only if you initiated the reconciliation session, otherwise ignore overlaps).
pub(crate) struct ReconciliationInitiator<
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
    Store,
    StoreCreationFunction,
    Fingerprint,
    PD,
    AT,
> {
    state: Rc<
        ReconciliationState<
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
            Store,
            StoreCreationFunction,
            Fingerprint,
            PD,
            AT,
        >,
    >,
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
        Store,
        StoreCreationFunction,
        StoreCreationError,
        Fingerprint,
        PD,
        AT,
    >
    ReconciliationInitiator<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
where
    StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
    Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
{
    /// Call this when we detected an overlap in PIO (and are the initiator of the WGPS session). This then sends the necessary message(s) for initiating RBSR on the overlap.
    pub async fn initiate_reconciliation(
        &mut self,
        overlap: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
        todo!("implement this")
    }
}

/// Use this struct to process all incoming messages on the reconciliation channel (via the `process_incoming_reconciliation_messages` method).
pub(crate) struct ReconciliationReceiver<
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
    Store,
    StoreCreationFunction,
    Fingerprint,
    PD,
    AT,
> {
    state: Rc<
        ReconciliationState<
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
            Store,
            StoreCreationFunction,
            Fingerprint,
            PD,
            AT,
        >,
    >,
    receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
    phantom: PhantomData<(Fingerprint, PD, AT)>,
    received_entry_buffer: Vec<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
    received_entry_buffer_capacity: usize,
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
        Store,
        StoreCreationFunction,
        StoreCreationError,
        Fingerprint,
        PD,
        AT,
    >
    ReconciliationReceiver<
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
        Store,
        StoreCreationFunction,
        Fingerprint,
        PD,
        AT,
    >
where
    N: Eq + core::hash::Hash + Clone,
    S: Clone,
    PD: Clone,
    AT: Clone,
    Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
    StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
{
    pub async fn process_incoming_reconciliation_messages(
        &mut self,
    ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
        loop {
            match self.decode_send_fp_or_announce_entries_msg().await? {
                Left(send_fp_msg) => {
                    todo!("act on fingerprint message");
                    continue; // go to next iteration of decoding loop
                }
                Right(announce_entries_msg) => {
                    let mut received_all_messages_for_this_range = announce_entries_msg.is_empty;
                    self.received_entry_buffer.clear();

                    if received_all_messages_for_this_range {
                        todo!("process announce_entries_msg.info.covers and is_final");
                        todo!("this is only inside an if-statement to get rid of dead-code warnings; do this unconditionally");
                    }

                    // Decode entries and their payloads until we got everything in the range
                    while !received_all_messages_for_this_range {
                        // Decode entries and their payload until a `ReconciliationTerminatePayload` msg with `is_final`
                        let send_entry_msg = self.decode_send_entry_msg_and_verify().await?;
                        let entry = send_entry_msg.entry.entry().entry();

                        let store = self
                            .state
                            .storedinator
                            .get_store(entry.namespace_id())
                            .await
                            .map_err(|store_creation_error| {
                                RbsrError::StoreCreation(store_creation_error)
                            })?;

                        match store
                            .ingest_entry(
                                send_entry_msg.entry.entry().clone(),
                                false,
                                EntryOrigin::Remote(self.state.session_id),
                            )
                            .await {
                                Ok(_) => {/* no-op */}
                                Err(EntryIngestionError::PruningPrevented) => panic!("Got a EntryIngestionError::PruningPrevented error despite calling Store::ingest_entry with prevent_pruning set to false. The store implementation is buggy."),
                                Err(EntryIngestionError::OperationsError(store_err)) => return Err(RbsrError::Store(store_err)),
                            }

                        if self.received_entry_buffer.len() < self.received_entry_buffer_capacity {
                            self.received_entry_buffer
                                .push(send_entry_msg.entry.clone());
                        }

                        // This flag ensures that receiving the payload for the same entry concurrently form multiple peers doesn't result in storing an incorrect payload.
                        let mut try_appending_payload = true;

                        loop {
                            match self.decode_send_payload_or_terminate_payload_msg().await? {
                                Left(send_payload_msg) => {
                                    if try_appending_payload {
                                        let mut bytes_to_append =
                                            FromSlice::new(&send_payload_msg.bytes[..]);
                                        match store
                                            .append_payload(
                                                entry.subspace_id(),
                                                entry.path(),
                                                Some(entry.payload_digest().clone()),
                                                Some(todo!()),
                                                &mut bytes_to_append,
                                            )
                                            .await
                                        {
                                            Ok(_) => { /* no-op */ }
                                            Err(PayloadAppendError::OperationError(store_err)) => {
                                                return Err(RbsrError::from(store_err));
                                            }
                                            Err(_) => {
                                                try_appending_payload = false;
                                            }
                                        }
                                    }

                                    // do nothing else, go to next iteration of the (inner)ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
                                }
                                Right(terminate_payload_msg) => {
                                    received_all_messages_for_this_range =
                                        terminate_payload_msg.is_final;
                                    break; // leaves the (inner) ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
                                }
                            }
                        }

                        todo!("reply with all entries (and their payloads if they are sufficiently small) in the range we have, except those we just received (may approximate the latter)");

                        // All done with this ReconciliationAnnounceEntries message, move on to the next iteration of the outer decoding loop.
                    }
                }
            }
        }
    }

    async fn decode_send_fp_or_announce_entries_msg(
        &mut self,
    ) -> Result<
        Either<
            ReconciliationSendFingerprint<MCL, MCC, MPL, S, Fingerprint>,
            ReconciliationAnnounceEntries<MCL, MCC, MPL, S>,
        >,
        RbsrError<CErr, StoreCreationError, Store::Error>,
    > {
        todo!()
    }

    async fn decode_send_entry_msg_and_verify(
        &mut self,
    ) -> Result<
        ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>,
        RbsrError<CErr, StoreCreationError, Store::Error>,
    > {
        todo!("verify message (resource handles) and (?) the entry itself");
    }

    async fn decode_send_payload_or_terminate_payload_msg(
        &mut self,
    ) -> Result<
        Either<ReconciliationSendPayload, ReconciliationTerminatePayload>,
        RbsrError<CErr, StoreCreationError, Store::Error>,
    > {
        todo!()
    }
}

pub(crate) enum RbsrError<SendingError, StoreCreationError, StoreErr> {
    Sending(SendingError),
    StoreCreation(StoreCreationError),
    Store(StoreErr),
}

impl<SendingError, StoreCreationError, StoreErr> From<StoreErr>
    for RbsrError<SendingError, StoreCreationError, StoreErr>
{
    fn from(value: StoreErr) -> Self {
        RbsrError::Store(value)
    }
}
