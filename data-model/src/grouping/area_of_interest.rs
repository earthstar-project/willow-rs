use crate::grouping::area::Area;

/// A grouping of [`crate::Entry`]s that are among the newest in some [store](https://willowprotocol.org/specs/data-model/index.html#store).
///
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#aois).
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct AreaOfInterest<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// To be included in this [`AreaOfInterest`], an [`crate::Entry`] must be included in the [`Area`].
    pub area: Area<MCL, MCC, MPL, S>,
    /// To be included in this AreaOfInterest, an Entry’s timestamp must be among the max_count greatest Timestamps, unless max_count is zero.
    pub max_count: u64,
    /// The total payload_lengths of all included Entries is at most max_size, unless max_size is zero.
    pub max_size: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> AreaOfInterest<MCL, MCC, MPL, S> {
    /// Creates a new [`AreaOfInterest`].
    pub fn new(area: Area<MCL, MCC, MPL, S>, max_count: u64, max_size: u64) -> Self {
        Self {
            area,
            max_count,
            max_size,
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> AreaOfInterest<MCL, MCC, MPL, S>
where
    S: PartialOrd + Clone,
{
    /// Returns the intersection of this [`AreaOfInterest`] with another.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#aoi_intersection).
    pub fn intersection(
        &self,
        other: &AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> Option<AreaOfInterest<MCL, MCC, MPL, S>> {
        self.area
            .intersection(&other.area)
            .map(|area_intersection| Self {
                area: area_intersection,
                max_count: core::cmp::min(self.max_count, other.max_count),
                max_size: self.max_size.min(other.max_size),
            })
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: Clone> From<Area<MCL, MCC, MPL, S>>
    for AreaOfInterest<MCL, MCC, MPL, S>
{
    fn from(value: Area<MCL, MCC, MPL, S>) -> Self {
        Self {
            area: value.clone(),
            max_count: 0,
            max_size: 0,
        }
    }
}
