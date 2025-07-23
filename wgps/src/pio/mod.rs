use std::{cell::RefCell, hash::Hash, ops::Deref};

use ufotofu::{BulkConsumer, Consumer, Producer};
use wb_async_utils::{
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    NamespaceId, Path, SubspaceId,
};
use willow_pio::PrivateInterest;

use crate::pio::capable_aois::InterestRegistry;

mod capable_aois;
mod salted_hashes;

/// The state for a PIO session.
///
/// This struct is opaque, but we expose it to allow for control over where it is allocated.
#[derive(Debug)]
pub struct State<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    P,
    PR,
    C,
    CR,
> where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    p: SharedProducer<PR, P>,
    c: SharedConsumer<CR, C>,
    interest_registry:
        RefCell<InterestRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap>>,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        ReadCap,
        P,
        PR,
        C,
        CR,
    > State<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, P, PR, C, CR>
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S> + Eq + Hash,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// Create a new opaque state for a PIO session.
    pub fn new(
        p: SharedProducer<PR, P>,
        c: SharedConsumer<CR, C>,
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
    ) -> Self {
        Self {
            p,
            c,
            interest_registry: RefCell::new(InterestRegistry::new(my_salt, h)),
        }
    }
}

/// Everything that makes up a single pio session.
struct PioSession<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap,
    P,
    PR,
    C,
    CR,
    StateRef: Deref<
        Target = State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            ReadCap,
            P,
            PR,
            C,
            CR,
        >,
    >,
> where
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    my_aoi_input: MyAoiInput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        P,
        PR,
        C,
        CR,
        StateRef,
    >,
    overlap_output: OverlappingAoiOutput,
    capability_handles: AoiHandles,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Hash,
        S: SubspaceId + Hash,
        ReadCap,
        P,
        PR,
        C,
        CR,
        StateRef,
    >
    PioSession<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        P,
        PR,
        C,
        CR,
        StateRef,
    >
where
    P: Producer,
    C: BulkConsumer<Item = u8, Final: Clone, Error: Clone>,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    StateRef: Deref<
            Target = State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                P,
                PR,
                C,
                CR,
            >,
        > + Clone,
{
    /// This function is the entry point to our implementation of private interest overlap detection.
    ///
    /// While the internals are somewhat complex, the outer interface of PIO is fairly simple: peers exchange PioBindReadCapability messages. Each of these is essentially an AreaOfInterest together with a read capability for that AoI.
    ///
    /// This function returns:
    ///
    /// - a [`MyAoiInput`], where you can put in the AoIs you'd like to sync (with backpressure applied based on the other peer's guarantees on the OverlapChannel),
    /// - an [`OverlappingAoiOutput`], a producer which emits the AoIs submitted both by the other peer and yourself for which there is an overlap, and
    /// - an [`AoiHandles`], which lets you resolve your and your peer's read capability handles to Aois.
    ///
    /// Internally, everything undergoes a bunch of cryptographic verification, and resource-handle-based backpressure.
    ///
    /// In the WGPS, peers can not only *submit* AoIs but also *revoke* them again (via ResourceHandleFree messages). This implementation punts on that functionality. You cannot undeclare any interest, and all requests by the other peer to remove an AoI will be ignored. This makes us selfish peers, and long-term this functionality should be implemented.
    pub fn new(state_ref: StateRef) -> Self {
        Self {
            my_aoi_input: MyAoiInput {
                session_state: state_ref.clone(),
            },
            overlap_output: todo!(),
            capability_handles: todo!(),
        }
    }
}

/// This is where you submit your own capable AoIs. More specifically, you asynchronously (backpressure based on the OverlapChannel) submit [`CapableAoi`]s. When a submitted AoI overlaps an AoI transmitted by the peer, both are emitted on the [`OverlappingAoiOutput`] corresponding to self.
pub(crate) struct MyAoiInput<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap,
    P,
    PR,
    C,
    CR,
    StateRef: Deref<
        Target = State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            ReadCap,
            P,
            PR,
            C,
            CR,
        >,
    >,
> where
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    session_state: StateRef,
}

pub(crate) struct OverlappingAoiOutput;

pub(crate) struct AoiHandles;

/// The information you submit to pio detection (basically the information you need for a `PioBindReadCapability` message).
#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CapableAoi<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap> {
    /// The read capability for the area one is interested in.
    capability: ReadCap,
    /// The max_count of the AreaOfInterest that the sender wants to sync.
    max_count: u64,
    /// The max_size of the AreaOfInterest that the sender wants to sync.
    max_size: u64,
    max_payload_power: u8,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap, N, S>
    CapableAoi<MCL, MCC, MPL, ReadCap>
where
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    N: NamespaceId,
    S: SubspaceId,
{
    fn to_private_interest(&self) -> PrivateInterest<MCL, MCC, MPL, N, S> {
        let area = self.capability.granted_area();
        PrivateInterest::new(
            self.capability.granted_namespace().clone(),
            area.subspace().clone(),
            area.path().clone(),
        )
    }
}

/// The semantics that a valid read capability must provide to be usable with the WGPS.
pub trait ReadCapability<const MCL: usize, const MCC: usize, const MPL: usize> {
    type Receiver;
    type NamespaceId;
    type SubspaceId;

    fn granted_area(&self) -> Area<MCL, MCC, MPL, Self::SubspaceId>;
    fn granted_namespace(&self) -> Self::NamespaceId;
}

/// The semantics that a valid enumeration capability must provide to be usable with the WGPS.
pub trait EnumerationCapability {
    type Receiver;
    type NamespaceId;

    fn granted_namespace(&self) -> Self::NamespaceId;
    fn receiver(&self) -> Self::Receiver;
}
