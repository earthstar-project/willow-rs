use arbitrary::Arbitrary;

use crate::{
    entry::{Entry, Timestamp},
    grouping::{area::Area, range::Range},
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

/// A three-dimensional range that includes every Entry included in all three of its ranges.
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#D3Range).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range3d<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId> {
    /// A range of [`SubspaceId`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#SubspaceRange).
    pub subspaces: Range<S>,
    /// A range of [`Path`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#PathRange).
    pub paths: Range<Path<MCL, MCC, MPL>>,
    /// A range of [`Timestamp`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#TimeRange).
    pub times: Range<Timestamp>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId>
    Range3d<MCL, MCC, MPL, S>
{
    /// Return whether an [`Entry`] is [included](https://willowprotocol.org/specs/grouping-entries/index.html#d3_range_include) by this 3d range.
    pub fn includes_entry<N: NamespaceId, PD: PayloadDigest>(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool {
        self.subspaces.includes(&entry.subspace_id)
            && self.paths.includes(&entry.path)
            && self.times.includes(&entry.timestamp)
    }

    /// Return the intersection between this [`Range3d`] and another.
    pub fn intersection(&self, other: &Range3d<MCL, MCC, MPL, S>) -> Option<Self> {
        let paths = self.paths.intersection(&other.paths)?;
        let times = self.times.intersection(&other.times)?;
        let subspaces = self.subspaces.intersection(&other.subspaces)?;
        Some(Self {
            paths,
            times,
            subspaces,
        })
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId> Default
    for Range3d<MCL, MCC, MPL, S>
{
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#default_3d_range).
    fn default() -> Self {
        Self {
            subspaces: Range::<S>::default(),
            paths: Range::new_open(Path::new_empty()),
            times: Range::new_open(0),
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: SubspaceId>
    From<Area<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
{
    fn from(_value: Area<MCL, MCC, MPL, S>) -> Self {
        todo!("Need to add successor fns to SubspaceId and Paths first.")
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, S> Arbitrary<'a>
    for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            subspaces: Arbitrary::arbitrary(u)?,
            paths: Arbitrary::arbitrary(u)?,
            times: Arbitrary::arbitrary(u)?,
        })
    }
}

#[cfg(test)]
mod tests {

    use crate::path::Component;

    use super::*;

    #[derive(Default, PartialEq, Eq, Clone)]
    struct FakeNamespaceId(usize);
    impl NamespaceId for FakeNamespaceId {}

    #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
    struct FakeSubspaceId(usize);
    impl SubspaceId for FakeSubspaceId {
        fn successor(&self) -> Option<Self> {
            Some(FakeSubspaceId(self.0 + 1))
        }
    }

    #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct FakePayloadDigest(usize);
    impl PayloadDigest for FakePayloadDigest {}

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn includes_entry() {
        let entry = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId(10),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            timestamp: 500,
            payload_length: 10,
            payload_digest: FakePayloadDigest::default(),
        };

        let range_including = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"b").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(range_including.includes_entry(&entry));

        let range_subspaces_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(11), FakeSubspaceId(13))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"b").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_subspaces_excluding.includes_entry(&entry));

        let range_paths_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_paths_excluding.includes_entry(&entry));

        let range_times_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"b").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(100, 200).unwrap(),
        };

        assert!(!range_times_excluding.includes_entry(&entry));
    }

    #[test]
    fn intersection() {
        let empty_intersection_subspace = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(11), FakeSubspaceId(13))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_subspace.is_none());

        let empty_intersection_path = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"4").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"6").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_path.is_none());

        let empty_intersection_time = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(200, 300).unwrap(),
        });

        assert!(empty_intersection_time.is_none());

        let intersection = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"6").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(2), FakeSubspaceId(4))
                .unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"3").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"9").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(50, 150).unwrap(),
        });

        assert!(intersection.is_some());

        assert_eq!(
            intersection.unwrap(),
            Range3d {
                subspaces: Range::<FakeSubspaceId>::new_closed(
                    FakeSubspaceId(2),
                    FakeSubspaceId(3)
                )
                .unwrap(),
                paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                    Path::new_from_slice(&[Component::new(b"3").unwrap()]).unwrap(),
                    Path::new_from_slice(&[Component::new(b"6").unwrap()]).unwrap(),
                )
                .unwrap(),
                times: Range::<Timestamp>::new_closed(50, 100).unwrap(),
            }
        )
    }
}
