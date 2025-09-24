use willow_data_model::grouping::{AreaOfInterest, NamespacedAreaMap};

use crate::{messages::PioBindReadCapability, ReadCapability};

pub(crate) struct OverlapFinder<const MCL: usize, const MCC: usize, const MPL: usize, N, S> {
    my_caois: NamespacedAreaMap<
        MCL,
        MCC,
        MPL,
        N,
        S,
        NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    >,
    their_caois: NamespacedAreaMap<
        MCL,
        MCC,
        MPL,
        N,
        S,
        NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    >,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S>
    OverlapFinder<MCL, MCC, MPL, N, S>
{
    pub(crate) fn new() -> Self {
        Self {
            my_caois: NamespacedAreaMap::default(),
            their_caois: NamespacedAreaMap::default(),
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S> OverlapFinder<MCL, MCC, MPL, N, S>
where
    N: Clone + Eq + std::hash::Hash,
    S: Clone + Eq + std::hash::Hash,
{
    pub(crate) fn add_mine(
        &mut self,
        to_add: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    ) -> Vec<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>> {
        self.add(to_add, true)
    }

    pub(crate) fn add_theirs(
        &mut self,
        to_add: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
    ) -> Vec<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>> {
        self.add(to_add, false)
    }

    fn add(
        &mut self,
        to_add: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
        is_mine: bool,
    ) -> Vec<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>> {
        let (inserted_into, other) = if is_mine {
            (&mut self.my_caois, &mut self.their_caois)
        } else {
            (&mut self.their_caois, &mut self.my_caois)
        };

        let the_namespace = to_add.namespace.clone();
        let the_area = to_add.aoi.area.clone();

        inserted_into.insert(&the_namespace, &the_area, to_add);

        other
            .intersecting_values(&the_namespace, &the_area)
            .into_iter()
            .map(NamespacedAoIWithMaxPayloadPower::clone)
            .collect()
    }
}

/// The sync-relevant information that remains of a `PioBindReadCapability` after performing cryptographic verification.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
pub struct NamespacedAoIWithMaxPayloadPower<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
> {
    pub namespace: N,
    pub aoi: AreaOfInterest<MCL, MCC, MPL, S>,
    pub max_payload_power: u8,
    pub my_handle: u64,
    pub their_handle: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S> Default
    for NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>
where
    N: Default,
{
    fn default() -> Self {
        Self {
            namespace: N::default(),
            aoi: AreaOfInterest::default(),
            max_payload_power: 0,
            my_handle: u64::MAX,
            their_handle: u64::MAX,
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S>
    NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>
where
    N: Clone,
    S: Clone,
{
    pub fn from_bind_read_capability_msg<
        ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    >(
        msg: &PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
    ) -> Self {
        Self {
            namespace: msg.capability.granted_namespace().clone(),
            aoi: AreaOfInterest {
                area: msg.capability.granted_area().clone(),
                max_count: msg.max_count,
                max_size: msg.max_size,
            },
            max_payload_power: msg.max_payload_power,
            my_handle: msg.receiver_handle,
            their_handle: msg.sender_handle,
        }
    }
}
