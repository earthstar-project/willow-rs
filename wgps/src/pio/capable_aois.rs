// Pure logic of ingesting authenticated aois and which pio messages to then send, based on which pio messages one has also receied in the meantime.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    iter::Extend,
    ops::Sub,
};

use either::Either::{self, Left, Right};
use multimap::MultiMap;
use willow_data_model::{grouping::Area, NamespaceId, PrivateInterest, SubspaceId};

use crate::pio::salted_hashes::{HashRegistry, Overlap, PioBindHashInformation};

/// Tell this struct about your own capable AOIs, and the PIO messages received from the other peer, and this struct tells you which messages to send and where there are overlaps of areas of interest. Does not perform any IO, only implements the logic of pio.
#[derive(Debug)]
pub(crate) struct InterestRegistry<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap,
> {
    hash_registry: HashRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S>,
    /// Tracks information associated with each PrivateInterest to which at least one of our submitted CapableAois has hashed.
    my_interests:
        HashMap<PrivateInterest<MCL, MCC, MPL, N, S>, PrivateInterestState<MCL, MCC, MPL, ReadCap>>,
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
    > InterestRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap>
where
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S> + Eq + Hash,
    N: NamespaceId,
    S: SubspaceId,
{
    pub fn new(
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
    ) -> Self {
        InterestRegistry {
            hash_registry: HashRegistry::new(my_salt, h),
            my_interests: HashMap::new(),
        }
    }

    /// Call this when you want to add a CapableAoi. Returns TODO
    pub(crate) fn submit_capable_aoi(
        &mut self,
        caoi: CapableAoi<MCL, MCC, MPL, ReadCap>,
    ) -> Option<PioBindHashInformation<INTEREST_HASH_LENGTH>> {
        // Convert to PrivateInterest, do salted hash stuff if the PrivateInterest is new (otherwise, deduplicate and do not report salted hash stuff).
        let p = caoi.to_private_interest();

        let p_is_fresh = self.my_interests.contains_key(&p);
        if p_is_fresh {
            // Do salted hash stuff!
            // We don't really care what that means, we simply delegate to `self.hash_registry` and forward the results.
            let salted_hash_stuff = self.hash_registry.submit_private_interest(&p);

            // Record information about this private interest (the `caoi` is the only CapableAoi that yielded it so far, and we have not yet detected any overlaps with the other peer's private interests).
            let mut caois = HashSet::new();
            caois.insert(caoi);
            self.my_interests.insert(p, PrivateInterestState { caois });

            return Some(salted_hash_stuff);
        } else {
            // No need to do salted hash stuff in this branch.

            let pi_state = self.my_interests.get_mut(&p).unwrap(); // can unwrap because `!p_is_fresh`

            // Add the new CapableAoi to the state of its PrivateInterest.
            pi_state.caois.insert(caoi);

            return None;
        }
    }

    /// Call this after sending both PioBindHash messages (only one if only a single hash and handle was returned by `submit_private_interest`).
    ///
    /// `fst_handle` is the overlap handle bound by the PioBindHash message with associated boolean `true`.
    ///
    /// `private_interest` is the PrivateInterest obtained via `caoi.to_private_interest()`, where `caoi` is the CapableAoi for which you called `submit_capable_aoi` (whose return value instructed you to call this method).
    ///
    /// Returns any detected matches.
    pub fn sent_pio_bind_hash_msgs(&mut self, fst_handle: u64) -> Vec<Overlap> {
        let overlaps = self.hash_registry.sent_pio_bind_hash_msgs(fst_handle);
        return overlaps;
    }

    /// Call this whenever we received a PioBindHash message. Returns any detected matches.
    pub fn received_pio_bind_hash_msg(
        &mut self,
        hash: [u8; INTEREST_HASH_LENGTH],
        actually_interested: bool,
    ) -> Vec<Overlap> {
        let overlaps = self
            .hash_registry
            .received_pio_bind_hash_msg(hash, actually_interested);

        return overlaps;
    }
}

#[derive(Debug)]
struct PrivateInterestState<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap> {
    /// All CapableAois we have [submitted](submit_capable_aoi) which resulted in the PrivateInterest of this state.
    caois: HashSet<CapableAoi<MCL, MCC, MPL, ReadCap>>,
}

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
