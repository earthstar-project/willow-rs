use crate::{grouping::area::Area, parameters::SubspaceId};

/// A grouping of [`Entry`]s that are among the newest in some [`Store`].
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#aois).
pub struct AreaOfInterest<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId> {
    /// To be included in this [`AreaOfInterest`], an [`Entry`] must be included in the [`Area`].
    pub area: Area<MCL, MCC, MPL, S>,
    /// To be included in this AreaOfInterest, an Entry’s timestamp must be among the max_count greatest Timestamps, unless max_count is zero.
    pub max_count: u64,
    /// The total payload_lengths of all included Entries is at most max_size, unless max_size is zero.
    pub max_size: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId>
    AreaOfInterest<MCL, MCC, MPL, S>
{
    /// Return the intersection of this [`AreaOfInterest`] with another.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#aoi_intersection).
    pub fn intersection(
        &self,
        other: AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> Option<AreaOfInterest<MCL, MCC, MPL, S>> {
        match self.area.intersection(&other.area) {
            None => None,
            Some(area_intersection) => Some(Self {
                area: area_intersection,
                max_count: core::cmp::min(self.max_count, other.max_count),
                max_size: self.max_size.min(other.max_size),
            }),
        }
    }
}
