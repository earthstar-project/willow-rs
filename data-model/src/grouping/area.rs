#[cfg(feature = "dev")]
use arbitrary::{size_hint::and_all, Arbitrary};

use crate::{
    entry::{Entry, Timestamp},
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

use super::range::Range;

/// The possible values of an [`Area`]'s `subspace`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum AreaSubspace<S: SubspaceId> {
    /// A value that signals that an [`Area`] includes Entries with arbitrary subspace_ids.
    Any,
    /// A concrete [`SubspaceId`].
    Id(S),
}

impl<S: SubspaceId> AreaSubspace<S> {
    /// Returns whether this [`AreaSubspace`] includes a given [`SubspaceId`].
    pub fn includes(&self, sub: &S) -> bool {
        match self {
            AreaSubspace::Any => true,
            AreaSubspace::Id(id) => sub == id,
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

    /// Returns whether this [`AreaSubspace`] includes Entries with arbitrary subspace_ids.
    pub fn is_any(&self) -> bool {
        matches!(self, AreaSubspace::Any)
    }
}

#[cfg(feature = "dev")]
impl<'a, S> Arbitrary<'a> for AreaSubspace<S>
where
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let is_any: bool = Arbitrary::arbitrary(u)?;

        if !is_any {
            let subspace: S = Arbitrary::arbitrary(u)?;

            return Ok(Self::Id(subspace));
        }

        Ok(Self::Any)
    }
}

/// A grouping of entries.
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#areas).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Area<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId> {
    /// To be included in this [`Area`], an [`Entry`]’s `subspace_id` must be equal to the subspace_id, unless it is any.
    subspace: AreaSubspace<S>,
    /// To be included in this [`Area`], an [`Entry`]’s `path` must be prefixed by the path.
    path: Path<MCL, MCC, MPL>,
    /// To be included in this [`Area`], an [`Entry`]’s `timestamp` must be included in the times.
    times: Range<Timestamp>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId> Area<MCL, MCC, MPL, S> {
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

    /// Returns an [`Area`] which includes all entries within a [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace).
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#subspace_area).
    pub fn new_subspace(sub: S) -> Self {
        Self {
            subspace: AreaSubspace::Id(sub),
            path: Path::new_empty(),
            times: Range::new_open(0),
        }
    }

    /// Returns whether an [`Area`] [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) an [`Entry`].
    pub fn includes_entry<N: NamespaceId, PD: PayloadDigest>(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool {
        self.subspace.includes(entry.subspace_id())
            && self.path.is_prefix_of(entry.path())
            && self.times.includes(&entry.timestamp())
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
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, S> Arbitrary<'a>
    for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let subspace: AreaSubspace<S> = Arbitrary::arbitrary(u)?;
        let path: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;
        let times: Range<u64> = Arbitrary::arbitrary(u)?;

        Ok(Self {
            subspace,
            path,
            times,
        })
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        and_all(&[
            AreaSubspace::<S>::size_hint(depth),
            Path::<MCL, MCC, MPL>::size_hint(depth),
            Range::<u64>::size_hint(depth),
        ])
    }
}

#[cfg(test)]
mod tests {

    use crate::path::Component;

    use super::*;

    #[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct FakeSubspaceId(usize);
    impl SubspaceId for FakeSubspaceId {
        fn successor(&self) -> Option<Self> {
            Some(FakeSubspaceId(self.0 + 1))
        }
    }

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn subspace_area_includes() {
        assert!(AreaSubspace::<FakeSubspaceId>::Any.includes(&FakeSubspaceId(5)));
        assert!(AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(5)).includes(&FakeSubspaceId(5)));
        assert!(!AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(8)).includes(&FakeSubspaceId(5)));
    }

    #[test]
    fn subspace_area_intersects() {
        // Non-empty intersections
        let any_any_intersection =
            AreaSubspace::<FakeSubspaceId>::Any.intersection(&AreaSubspace::<FakeSubspaceId>::Any);

        assert!(any_any_intersection.is_some());

        assert!(any_any_intersection.unwrap() == AreaSubspace::<FakeSubspaceId>::Any);

        let any_id_intersection = AreaSubspace::<FakeSubspaceId>::Any
            .intersection(&AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6)));

        assert!(any_id_intersection.is_some());

        assert!(
            any_id_intersection.unwrap() == AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6))
        );

        let id_id_intersection = AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6))
            .intersection(&AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6)));

        assert!(id_id_intersection.is_some());

        assert!(
            id_id_intersection.unwrap() == AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6))
        );

        // Empty intersections

        let empty_id_id_intersection = AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(7))
            .intersection(&AreaSubspace::<FakeSubspaceId>::Id(FakeSubspaceId(6)));

        assert!(empty_id_id_intersection.is_none())
    }

    #[test]
    fn area_full() {
        let full_area = Area::<MCL, MCC, MPL, FakeSubspaceId>::new_full();

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
        let subspace_area = Area::<MCL, MCC, MPL, FakeSubspaceId>::new_subspace(FakeSubspaceId(7));

        assert_eq!(
            subspace_area,
            Area {
                subspace: AreaSubspace::Id(FakeSubspaceId(7)),
                path: Path::new_empty(),
                times: Range::new_open(0)
            }
        )
    }

    #[test]
    fn area_intersects() {
        let empty_intersection_subspace = Area::<MCL, MCC, MPL, FakeSubspaceId> {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
            path: Path::new_empty(),
            times: Range::new_open(0),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(FakeSubspaceId(2)),
            path: Path::new_empty(),
            times: Range::new_open(0),
        });

        assert!(empty_intersection_subspace.is_none());

        let empty_intersection_path = Area::<MCL, MCC, MPL, FakeSubspaceId> {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
            path: Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
            times: Range::new_open(0),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
            path: Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            times: Range::new_open(0),
        });

        assert!(empty_intersection_path.is_none());

        let empty_intersection_time = Area::<MCL, MCC, MPL, FakeSubspaceId> {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
            path: Path::new_empty(),
            times: Range::new_closed(0, 1).unwrap(),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
            path: Path::new_empty(),
            times: Range::new_closed(2, 3).unwrap(),
        });

        assert!(empty_intersection_time.is_none());

        let intersection = Area::<MCL, MCC, MPL, FakeSubspaceId> {
            subspace: AreaSubspace::Any,
            path: Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            times: Range::new_closed(0, 10).unwrap(),
        }
        .intersection(&Area {
            subspace: AreaSubspace::Id(FakeSubspaceId(1)),
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
                subspace: AreaSubspace::Id(FakeSubspaceId(1)),
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
