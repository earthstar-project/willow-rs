// Pure logic of ingesting authenticated aois and which pio messages to then send, based on which pio messages one has also receied in the meantime.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    iter::Extend,
};

use either::Either::{self, Left, Right};
use willow_data_model::{NamespaceId, SubspaceId};
use willow_pio::PrivateInterest;

use crate::{
    messages::PioBindReadCapability,
    pio::{
        salted_hashes::{HashRegistry, MyInterestingHandleInfo, Overlap, PioBindHashInformation},
        CapableAoi,
    },
    ReadCapability,
};

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
    EnumCap,
> {
    pub hash_registry: HashRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S>,
    /// Tracks information associated with each PrivateInterest to which at least one of our submitted CapableAois has hashed.
    my_interests: HashMap<
        PrivateInterest<MCL, MCC, MPL, N, S>,
        PrivateInterestState<MCL, MCC, MPL, ReadCap, EnumCap>,
    >,
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
        EnumCap,
    > InterestRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap>
where
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S> + Eq + Hash,
    EnumCap: Eq + Hash,
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

    /// Call this when you want to add a CapableAoi.
    pub(crate) fn submit_capable_aoi(
        &mut self,
        caoi: CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>,
    ) -> Either<PioBindHashInformation<INTEREST_HASH_LENGTH>, &HashSet<Overlap>> {
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
            self.my_interests.insert(
                p,
                PrivateInterestState {
                    caois,
                    overlaps: HashSet::new(),
                },
            );

            return Left(salted_hash_stuff);
        } else {
            // No need to do salted hash stuff in this branch.

            let pi_state = self.my_interests.get_mut(&p).unwrap(); // can unwrap because `!p_is_fresh`

            // Add the new CapableAoi to the state of its PrivateInterest...
            pi_state.caois.insert(caoi);

            // ...and report all overlaps.
            return Right(&pi_state.overlaps);
        }
    }

    /// Call this after sending both PioBindHash messages (only one if only a single hash and handle was returned by `submit_private_interest`).
    ///
    /// `fst_handle` is the overlap handle bound by the PioBindHash message with associated boolean `true`.
    ///
    /// `private_interest` is the PrivateInterest obtained via `caoi.to_private_interest()`, where `caoi` is the CapableAoi for which you called `submit_capable_aoi` (whose return value instructed you to call this method).
    ///
    /// Returns any detected matches.
    pub fn sent_pio_bind_hash_msgs(
        &mut self,
        fst_handle: u64,
        private_interest: &PrivateInterest<MCL, MCC, MPL, N, S>,
    ) -> Vec<Overlap> {
        let overlaps = self.hash_registry.sent_pio_bind_hash_msgs(fst_handle);

        let pi_state = self.my_interests.get_mut(private_interest).unwrap();
        pi_state.overlaps.extend(overlaps.iter());

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

        // Add the overlap information to `self.my_interests`.
        for overlap in overlaps.iter() {
            let private_interest_info = self
                .hash_registry
                .get_interesting_handle_info(overlap.my_interesting_handle);
            let pi_state = self
                .my_interests
                .get_mut(&private_interest_info.private_interest)
                .unwrap();
            pi_state.overlaps.insert(*overlap);
        }

        return overlaps;
    }

    /// Call this whenever we received a PioAnnounceOverlap message. Returns the indicated overlap, or an error if the message data was invalid.
    pub fn received_pio_announce_overlap_msg(
        &mut self,
        their_handle: u64,
        my_handle: u64,
        authentication: &[u8; INTEREST_HASH_LENGTH],
        enum_cap: &Option<EnumCap>,
    ) -> Result<Overlap, ReceivedAnnouncementError> {
        // Retrieve information stored with my_handle, error if they provided a fake one.
        match self.hash_registry.try_get_our_handle_info(my_handle) {
            None => return Err(ReceivedAnnouncementError::UnboundMyHandle),
            Some((MyInterestingHandleInfo { private_interest }, my_interesting_handle)) => {
                // Our handle is real!
                let was_primary = my_interesting_handle == my_handle;

                // We can verify their authentication.
                let expected_authentication =
                    (self.hash_registry.h)(private_interest, &self.hash_registry.their_salt());

                if authentication != &expected_authentication {
                    return Err(ReceivedAnnouncementError::InvalidAuthentication);
                }

                // Next, verify that `their_handle` is valid and corresponds to the same hash as `my_handle`.
                match self.hash_registry.get_their_handle(their_handle) {
                    None => return Err(ReceivedAnnouncementError::UnboundTheirHandle),
                    Some((their_hash, their_primary)) => {
                        // Their handle is real!

                        // Does is indeed correspond to the same hash as our handle?
                        let expected_hash =
                            (self.hash_registry.h)(private_interest, &self.hash_registry.my_salt);

                        if their_hash != &expected_hash {
                            return Err(ReceivedAnnouncementError::MismatchedHandles);
                        }

                        // Now, check whether there is an enum_cap and there *should* be one.
                        let awkward = !was_primary && !*their_primary;
                        if awkward && enum_cap.is_none() {
                            return Err(ReceivedAnnouncementError::MissingEnumerationCapability);
                        }
                        // Nothing to do with the enum_cap beyond checking its existence, oddly enough: its validity was checked during decoding, its receiver and namespace were checked during decoding (the namespace being taken from the same `private_interest` as we retrieved above) as well.

                        // All checks passed. Now we can notify our surrounding code about the overlap this message informs us about.
                        let pi_state = self.my_interests.get_mut(private_interest).unwrap();

                        let overlap = Overlap {
                            should_request_capability: false,
                            should_send_capability: true,
                            awkward: awkward,
                            my_interesting_handle: my_interesting_handle,
                            their_handle: their_handle,
                        };

                        pi_state.overlaps.insert(overlap);

                        return Ok(overlap);
                    }
                }
            }
        }
    }

    /// Call this whenever we received a PioBindReadCapability message. Returns the indicated overlap, or an error if the message data was invalid.
    pub fn received_pio_bind_read_capability_msg<RC: ReadCapability<MCL, MCC, MPL>>(
        &mut self,
        msg: &PioBindReadCapability<MCL, MCC, MPL, RC>,
    ) -> Result<Overlap, ReceivedBindCapabilityError> {
        let my_handle = msg.receiver_handle;
        let their_handle = msg.sender_handle;

        // Retrieve information stored with my_handle, error if they provided a fake one.
        match self.hash_registry.try_get_our_handle_info(my_handle) {
            None => return Err(ReceivedBindCapabilityError::UnboundMyHandle),
            Some((MyInterestingHandleInfo { private_interest }, my_interesting_handle)) => {
                // Our handle is real!

                // Next, verify that `their_handle` is valid and corresponds to the same hash as `my_handle`.
                match self.hash_registry.get_their_handle(their_handle) {
                    None => return Err(ReceivedBindCapabilityError::UnboundTheirHandle),
                    Some((their_hash, _their_primary)) => {
                        // Their handle is real!

                        // Does is indeed correspond to the same hash as our handle?
                        let expected_hash =
                            (self.hash_registry.h)(private_interest, &self.hash_registry.my_salt);

                        if their_hash != &expected_hash {
                            return Err(ReceivedBindCapabilityError::MismatchedHandles);
                        }

                        // All checks passed. Now we can notify our surrounding code about the overlap this message informs us about.
                        let pi_state = self.my_interests.get_mut(private_interest).unwrap();

                        let overlap = Overlap {
                            should_request_capability: false,
                            should_send_capability: true,
                            awkward: false, // whatever, unused
                            my_interesting_handle: my_interesting_handle,
                            their_handle: their_handle,
                        };

                        pi_state.overlaps.insert(overlap);

                        return Ok(overlap);
                    }
                }
            }
        }
    }

    /// `my_handle` must be bound by us, panics otherwise!
    pub(crate) fn caois_for_my_interesting_handle(
        &self,
        my_handle: u64,
    ) -> &HashSet<CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>> {
        let handle_info = self.hash_registry.get_interesting_handle_info(my_handle);
        let pi_state = self
            .my_interests
            .get(&handle_info.private_interest)
            .unwrap();
        &pi_state.caois
    }

    /// `my_handle` must be bound by us, panics otherwise!
    pub(crate) fn private_interest_for_my_interesting_handle(
        &self,
        my_handle: u64,
    ) -> &PrivateInterest<MCL, MCC, MPL, N, S> {
        let handle_info = self.hash_registry.get_interesting_handle_info(my_handle);
        &handle_info.private_interest
    }
}

pub(crate) enum ReceivedAnnouncementError {
    /// The peer's message claimed we had bound a certain OverlapHandle, but we did not.
    UnboundMyHandle,
    /// The peer's message claimed they had bound a certain OverlapHandle, but they did not.
    UnboundTheirHandle,
    /// The peer's PioAnnounceOverlap message contained an invalid `authentication`.
    InvalidAuthentication,
    /// Their handle and my my handle did not correspond to equal private interests.
    MismatchedHandles,
    /// Their message should have had an enumeration capability, but it didn't.
    MissingEnumerationCapability,
}

pub(crate) enum ReceivedBindCapabilityError {
    /// The peer's message claimed we had bound a certain OverlapHandle, but we did not.
    UnboundMyHandle,
    /// The peer's message claimed they had bound a certain OverlapHandle, but they did not.
    UnboundTheirHandle,
    /// Their handle and my my handle did not correspond to equal private interests.
    MismatchedHandles,
}

#[derive(Debug)]
pub(crate) struct PrivateInterestState<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    ReadCap,
    EnumCap,
> {
    /// All CapableAois we have [submitted](submit_capable_aoi) which resulted in the PrivateInterest of this state.
    caois: HashSet<CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>>,
    /// All overlaps with this Private Interest and PrivateInterests registered by the other peer.
    overlaps: HashSet<Overlap>,
}
