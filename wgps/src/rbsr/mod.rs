use std::rc::Rc;

use lcmux::ChannelSender;
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
    ) -> Self {
        let state = Rc::new(ReconciliationState::new(
            pio_state,
            storedinator,
            Mutex::new(reconciliation_channel_sender),
        ));

        Self {
            initiator: ReconciliationInitiator { state: state },
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

pub(crate) enum RbsrError<SendingError> {
    Sending(SendingError),
}
