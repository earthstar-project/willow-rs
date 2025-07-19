#[cfg(feature = "dev")]
use crate::{grouping::RangeEnd, parameters::SubspaceId, PathBuilder};
#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use ufotofu_codec::Encodable;
use ufotofu_codec_endian::U64BE;

use crate::{
    entry::{Entry, Timestamp},
    path::Path,
};

use super::range::Range;

/// The possible values of an [`Area`]'s `subspace`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub enum AreaSubspace<S> {
    /// A value that signals that an [`Area`] includes Entries with arbitrary subspace_ids.
    Any,
    /// A concrete [`SubspaceId`].
    Id(S),
}

impl<S> AreaSubspace<S> {
    /// Returns whether this [`AreaSubspace`] includes Entries with arbitrary subspace_ids.
    pub fn is_any(&self) -> bool {
        matches!(self, AreaSubspace::Any)
    }
}

impl<S> AreaSubspace<S>
where
    S: PartialEq,
{
    /// Returns whether this [`AreaSubspace`] includes a given [`SubspaceId`].
    pub fn includes(&self, sub: &S) -> bool {
        match self {
            AreaSubspace::Any => true,
            AreaSubspace::Id(id) => sub == id,
        }
    }
}

impl<S> AreaSubspace<S>
where
    S: PartialEq + Clone,
{
    /// Returns whether this [`AreaSubspace`] includes a given [`AreaSubspace`].
    pub fn includes_area_subspace(&self, other: &Self) -> bool {
        match (self, other) {
            (AreaSubspace::Any, AreaSubspace::Any) => true,
            (AreaSubspace::Any, AreaSubspace::Id(_)) => true,
            (AreaSubspace::Id(_), AreaSubspace::Any) => false,
            (AreaSubspace::Id(id), AreaSubspace::Id(id_other)) => id == id_other,
        }
    }

    /// Returns the intersection between two [`AreaSubspace`].
    fn intersection(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::Any, Self::Any) => Some(Self::Any),
            (Self::Id(a), Self::Any) => Some(Self::Id(a.clone())),
            (Self::Any, Self::Id(b)) => Some(Self::Id(b.clone())),
            (Self::Id(a), Self::Id(b)) if a == b => Some(Self::Id(a.clone())),
            (Self::Id(_a), Self::Id(_b)) => None,
        }
    }
}

impl<S: PartialEq> PartialEq<S> for AreaSubspace<S> {
    fn eq(&self, other: &S) -> bool {
        match self {
            AreaSubspace::Any => false,
            AreaSubspace::Id(s) => s == other,
        }
    }
}

/// A grouping of entries.
///
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#areas).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct Area<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// To be included in this [`Area`], an [`Entry`]’s `subspace_id` must be equal to the subspace_id, unless it is any.
    subspace: AreaSubspace<S>,
    /// To be included in this [`Area`], an [`Entry`]’s `path` must be prefixed by the path.
    path: Path<MCL, MCC, MPL>,
    /// To be included in this [`Area`], an [`Entry`]’s `timestamp` must be included in the times.
    times: Range<Timestamp>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Area<MCL, MCC, MPL, S> {
    /// Creates a new [`Area`].
    pub fn new(
        subspace: AreaSubspace<S>,
        path: Path<MCL, MCC, MPL>,
        times: Range<Timestamp>,
    ) -> Self {
        Area {
            subspace,
            path,
            times,
        }
    }

    /// Returns a reference to the [`AreaSubspace`].
    ///
    /// To be included in this [`Area`], an [`Entry`]’s `subspace_id` must be equal to the subspace_id, unless it is any.
    pub fn subspace(&self) -> &AreaSubspace<S> {
        &self.subspace
    }

    /// Returns a reference to the [`Path`].
    ///
    /// To be included in this [`Area`], an [`Entry`]’s `path` must be prefixed by the path.
    pub fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.path
    }

    /// Returns a reference to the range of [`Timestamp`]s.
    ///
    /// To be included in this [`Area`], an [`Entry`]’s `timestamp` must be included in the times.
    pub fn times(&self) -> &Range<Timestamp> {
        &self.times
    }

    /// Returns an [`Area`] which includes all entries.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#full_area).
    pub fn new_full() -> Self {
        Self {
            subspace: AreaSubspace::Any,
            path: Path::new_empty(),
            times: Range::new_open(0),
        }
    }

    /// Returns an [`Area`] which includes all entries within a given [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace).
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#subspace_area).
    pub fn new_subspace(sub: S) -> Self {
        Self {
            subspace: AreaSubspace::Id(sub),
            path: Path::new_empty(),
            times: Range::new_open(0),
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Area<MCL, MCC, MPL, S>
where
    S: PartialEq,
{
    /// Returns whether an [`Area`] [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) an [`Entry`].
    pub fn includes_entry<N, PD>(&self, entry: &Entry<MCL, MCC, MPL, N, S, PD>) -> bool {
        self.includes_triplet(entry.subspace_id(), entry.path(), entry.timestamp())
    }

    /// Returns whether an [`Area`] [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) includes an entry with a given subspace_id, path, and timestamp.
    pub fn includes_triplet(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        timestamp: Timestamp,
    ) -> bool {
        self.subspace.includes(subspace_id)
            && self.path.is_prefix_of(path)
            && self.times.includes(&timestamp)
    }

    /// Returns whether an [`Area`] fully [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) another [`Area`].
    pub fn includes_area(&self, area: &Self) -> bool {
        match (&self.subspace, &area.subspace) {
            (AreaSubspace::Any, AreaSubspace::Any) => {
                self.path.is_prefix_of(&area.path) && self.times.includes_range(&area.times)
            }
            (AreaSubspace::Any, AreaSubspace::Id(_)) => {
                self.path.is_prefix_of(&area.path) && self.times.includes_range(&area.times)
            }
            (AreaSubspace::Id(_), AreaSubspace::Any) => false,
            (AreaSubspace::Id(subspace_a), AreaSubspace::Id(subspace_b)) => {
                subspace_a == subspace_b
                    && self.path.is_prefix_of(&area.path)
                    && self.times.includes_range(&area.times)
            }
        }
    }

    /// Returns whether an [`Entry`] can cause prefix-pruning in this [`Area`].
    pub fn could_be_pruned_by<N, PD>(&self, entry: &Entry<MCL, MCC, MPL, N, S, PD>) -> bool {
        self.could_be_pruned_by_triplet(entry.subspace_id(), entry.path(), entry.timestamp())
    }

    /// Returns whether an [`Entry`] of the given subspace_id, path, and timestamp can cause prefix-pruning in this [`Area`].
    pub fn could_be_pruned_by_triplet(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        timestamp: Timestamp,
    ) -> bool {
        if let AreaSubspace::Id(my_subspace_id) = self.subspace() {
            if my_subspace_id != subspace_id {
                return false;
            }
        }

        if timestamp < self.times().start {
            return false;
        }

        path.is_prefix_of(&self.path) || path.is_prefixed_by(&self.path)
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Area<MCL, MCC, MPL, S>
where
    S: PartialEq + Clone,
{
    /// Returns the intersection of this [`Area`] with another.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#area_intersection).
    pub fn intersection(&self, other: &Area<MCL, MCC, MPL, S>) -> Option<Self> {
        let subspace_id = self.subspace.intersection(&other.subspace)?;
        let path = if self.path.is_prefix_of(&other.path) {
            Some(other.path.clone())
        } else if self.path.is_prefixed_by(&other.path) {
            Some(self.path.clone())
        } else {
            None
        }?;
        let times = self.times.intersection(&other.times)?;
        Some(Self {
            subspace: subspace_id,
            times,
            path,
        })
    }

    /// Returns whether an [`Area`] _almost includes_ another area, that is, if the other [`Area`] would be included by this if [`Area] if it had the same [`SubspaceId`].
    pub fn almost_includes_area(&self, other: &Area<MCL, MCC, MPL, S>) -> bool {
        let subspace_is_fine = match (self.subspace(), other.subspace()) {
            (AreaSubspace::Id(self_id), AreaSubspace::Id(other_id)) => self_id == other_id,
            _ => true,
        };
        subspace_is_fine
            && self.path.is_prefix_of(&other.path)
            && self.times.includes_range(&other.times)
    }
}

#[cfg(feature = "dev")]
pub fn arbitrary_included_area<'a, const MCL: usize, const MCC: usize, const MPL: usize, S>(
    area: &Area<MCL, MCC, MPL, S>,
    u: &mut arbitrary::Unstructured<'a>,
) -> arbitrary::Result<Area<MCL, MCC, MPL, S>>
where
    S: SubspaceId + Arbitrary<'a>,
{
    let suffix: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;

    let total_length = area.path().path_length() + suffix.path_length();
    let total_component_length = area.path().component_count() + suffix.component_count();

    let included_path = if let Ok(mut builder) = PathBuilder::new_from_prefix(
        total_length,
        total_component_length,
        area.path(),
        area.path.component_count(),
    ) {
        for component in suffix.components() {
            builder.append_component(component)
        }

        builder.build()
    } else {
        area.path().clone()
    };

    let subspace = match area.subspace() {
        AreaSubspace::Any => {
            let is_subspace: bool = Arbitrary::arbitrary(u)?;

            if is_subspace {
                let subspace: S = Arbitrary::arbitrary(u)?;
                AreaSubspace::Id(subspace)
            } else {
                AreaSubspace::Any
            }
        }
        AreaSubspace::Id(id) => AreaSubspace::Id(id.clone()),
    };

    let start_offset: u64 = Arbitrary::arbitrary(u)?;

    let new_start = area
        .times()
        .start
        .checked_add(start_offset)
        .map_or(area.times().start, |res| res);

    let end_offset: u64 = Arbitrary::arbitrary(u)?;

    let new_end = match area.times().end {
        crate::grouping::RangeEnd::Closed(end) => {
            RangeEnd::Closed(end.checked_sub(end_offset).map_or(end, |res| res))
        }
        crate::grouping::RangeEnd::Open => {
            let is_any: bool = Arbitrary::arbitrary(u)?;

            if is_any {
                RangeEnd::Open
            } else {
                RangeEnd::Closed(end_offset)
            }
        }
    };

    let times = if new_start <= new_end {
        Range::new(new_start, new_end)
    } else {
        *area.times()
    };

    Ok(Area::new(subspace, included_path, times))
}

/// This is an "inofficial" encoding not on the encodings spec page.
///
/// ## An Absolute Encoding Relation for Area
///
/// - First byte is a header of bitflags:
///     - most significant bit: `1` iff the subspace is `any`.
///     - third-most significant bit: `1` iff the timestamp range is open.
///     - remaining six bits: arbitrary.
/// - encoding of the subspace id, or empty string if the subspace is `any`
/// - encoding of the path
/// - encoding of the start of the timestamp range as an 8-byte big-endian integer
/// - encoding of the end of the timestamp range as an 8-byte big-endian integer, or empty string if the timestamp range is open
impl<const MCL: usize, const MCC: usize, const MPL: usize, S: Encodable> Encodable
    for Area<MCL, MCC, MPL, S>
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0;

        if self.subspace().is_any() {
            header |= 0b1000_0000;
        }
        if self.times().is_open() {
            header |= 0b0100_0000;
        }

        consumer.consume(header).await?;

        if let AreaSubspace::Id(id) = self.subspace() {
            id.encode(consumer).await?;
        }

        self.path().encode(consumer).await?;

        U64BE(self.times().start).encode(consumer).await?;
        if let Some(end) = self.times().get_end() {
            U64BE(*end).encode(consumer).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::path::Component;

    use super::*;

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn subspace_area_includes() {
        assert!(AreaSubspace::<u64>::Any.includes(&5));
        assert!(AreaSubspace::<u64>::Id(5).includes(&5));
        assert!(!AreaSubspace::<u64>::Id(8).includes(&5));
    }

    #[test]
    fn subspace_area_intersects() {
        // Non-empty intersections
        let any_any_intersection = AreaSubspace::<u64>::Any.intersection(&AreaSubspace::<u64>::Any);

        assert!(any_any_intersection.is_some());

        assert!(any_any_intersection.unwrap() == AreaSubspace::<u64>::Any);

        let any_id_intersection =
            AreaSubspace::<u64>::Any.intersection(&AreaSubspace::<u64>::Id(6));

        assert!(any_id_intersection.is_some());

        assert!(any_id_intersection.unwrap() == AreaSubspace::<u64>::Id(6));

        let id_id_intersection =
            AreaSubspace::<u64>::Id(6).intersection(&AreaSubspace::<u64>::Id(6));

        assert!(id_id_intersection.is_some());

        assert!(id_id_intersection.unwrap() == AreaSubspace::<u64>::Id(6));

        // Empty intersections

        let empty_id_id_intersection =
            AreaSubspace::<u64>::Id(7).intersection(&AreaSubspace::<u64>::Id(6));

        assert!(empty_id_id_intersection.is_none())
    }

    #[test]
    fn area_full() {
        let full_area = Area::<MCL, MCC, MPL, u64>::new_full();

        assert_eq!(
            full_area,
            Area {
                subspace: AreaSubspace::Any,
                path: Path::new_empty(),
                times: Range::new_open(0)
            }
        )
    }

    #[test]
    fn area_subspace() {
        let subspace_area = Area::<MCL, MCC, MPL, u64>::new_subspace(7);

        assert_eq!(
            subspace_area,
            Area {
                subspace: AreaSubspace::Id(7),
                path: Path::new_empty(),
                times: Range::new_open(0)
            }
        )
    }

    #[test]
    fn area_intersects() {
        let empty_intersection_subspace = Area::<MCL, MCC, MPL, u64> {
            subspace: AreaSubspace::Id(1),
            path: Path::new_empty(),
            times: Range::new_open(0),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(2),
            path: Path::new_empty(),
            times: Range::new_open(0),
        });

        assert!(empty_intersection_subspace.is_none());

        let empty_intersection_path = Area::<MCL, MCC, MPL, u64> {
            subspace: AreaSubspace::Id(1),
            path: Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
            times: Range::new_open(0),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(1),
            path: Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            times: Range::new_open(0),
        });

        assert!(empty_intersection_path.is_none());

        let empty_intersection_time = Area::<MCL, MCC, MPL, u64> {
            subspace: AreaSubspace::Id(1),
            path: Path::new_empty(),
            times: Range::new_closed(0, 1).unwrap(),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(1),
            path: Path::new_empty(),
            times: Range::new_closed(2, 3).unwrap(),
        });

        assert!(empty_intersection_time.is_none());

        let intersection = Area::<MCL, MCC, MPL, u64> {
            subspace: AreaSubspace::Any,
            path: Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            times: Range::new_closed(0, 10).unwrap(),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(1),
            path: Path::new_from_slice(&[
                Component::new(b"1").unwrap(),
                Component::new(b"2").unwrap(),
            ])
            .unwrap(),
            times: Range::new_closed(5, 15).unwrap(),
        });

        assert!(intersection.is_some());

        assert_eq!(
            intersection.unwrap(),
            Area {
                subspace: AreaSubspace::Id(1),
                path: Path::new_from_slice(&[
                    Component::new(b"1").unwrap(),
                    Component::new(b"2").unwrap(),
                ])
                .unwrap(),
                times: Range::new_closed(5, 10).unwrap(),
            }
        )
    }
}
