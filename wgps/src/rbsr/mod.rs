use std::rc::Rc;

use lcmux::{ChannelReceiver, ChannelSender};
use wb_async_utils::Mutex;

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
    ) -> Self {
        Self {
            pio_state,
            storedinator,
            reconciliation_channel_sender,
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
    >
{
    /// The entrypoint to reconciliation. Use the public fields of this struct for the separate sub-task.
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
    ) -> Self {
        let state = Rc::new(ReconciliationState::new(
            pio_state,
            storedinator,
            Mutex::new(reconciliation_channel_sender),
        ));

        Self {
            initiator: ReconciliationInitiator {
                state: state.clone(),
            },
            receiver: ReconciliationReceiver {
                state: state,
                receiver: reconciliation_channel_receiver,
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
    >
{
    /// Call this when we detected an overlap in PIO (and are the initiator of the WGPS session). This then sends the necessary message(s) for initiating RBSR on the overlap.
    pub async fn initiate_reconciliation(
        &mut self,
        overlap: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    ) -> Result<(), RbsrError<CErr>> {
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
        >,
    >,
    receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
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
    >
{
    pub async fn process_incoming_reconciliation_messages(
        &mut self,
    ) -> Result<(), RbsrError<CErr>> {
        loop {
            match todo!("decode ReconciliationSendFingerprint or ReconciliationAnnounceEntries") {
                ReconciliationSendFingerprint => {
                    todo!("act on fingerprint message");
                    continue; // go to next iteration of decoding loop
                }
                ReconciliationAnnounceEntries => {
                    let received_all_messages_for_this_range: bool = todo!("msg.is_empty");

                    // Decode entries and their payloads until we got everything in the range
                    while !received_all_messages_for_this_range {
                        // Decode entries and their payload until a `ReconciliationTerminatePayload` msg with `is_final`
                        todo!("decode ReconciliationSendEntry");
                        todo!("add entry to our store");

                        loop {
                            match todo!("decode ReconciliationSendPayload or ReconciliationTerminatePayload") {
                                ReconciliationSendPayload=> {
                                    todo!("append payload to our store (ensure that the entry matches and we append at the correct offset)");
                                    // do nothing else, go to next iteration of the ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
                                }
                                ReconciliationTerminatePayload => {
                                    received_all_messages_for_this_range = todo!("msg.is_final");
                                }
                            }
                        }

                        todo!("reply with all entries (and their payloads if they are sufficiently small) in the range we have, except those we just received (may approximate the latter)");
                        // All done with this ReconciliationAnnounceEntries message, move on to the next iteration of the outer decoding loop.
                    }
                }
            }
        }
        todo!("implement this")
    }
}

pub(crate) enum RbsrError<SendingError> {
    Sending(SendingError),
}
