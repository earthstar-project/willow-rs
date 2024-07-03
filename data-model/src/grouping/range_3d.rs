use crate::{
    entry::{Entry, Timestamp},
    grouping::{area::Area, range::Range},
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

/// A three-dimensional range that includes every Entry included in all three of its ranges.
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#D3Range).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range3d<S: SubspaceId, P: Path> {
    /// A range of [`SubspaceId`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#SubspaceRange).
    pub subspaces: Range<S>,
    /// A range of [`Path`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#PathRange).
    pub paths: Range<P>,
    /// A range of [`Timestamp`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#TimeRange).
    pub times: Range<Timestamp>,
}

impl<S: SubspaceId, P: Path> Range3d<S, P> {
    /// Return whether an [`Entry`] is [included](https://willowprotocol.org/specs/grouping-entries/index.html#d3_range_include) by this 3d range.
    pub fn includes_entry<N: NamespaceId, PD: PayloadDigest>(
        &self,
        entry: &Entry<N, S, P, PD>,
    ) -> bool {
        self.subspaces.includes(&entry.subspace_id)
            && self.paths.includes(&entry.path)
            && self.times.includes(&entry.timestamp)
    }

    /// Return the intersection between this [`Range3d`] and another.
    pub fn intersection(&self, other: &Range3d<S, P>) -> Option<Self> {
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

impl<S: SubspaceId, P: Path> Default for Range3d<S, P> {
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#default_3d_range).
    fn default() -> Self {
        Self {
            subspaces: Range::<S>::default(),
            paths: Range::new_open(P::empty()),
            times: Range::new_open(0),
        }
    }
}

impl<S: SubspaceId, P: Path> From<Area<S, P>> for Range3d<S, P> {
    fn from(_value: Area<S, P>) -> Self {
        todo!("Need to add successor fns to SubspaceId and Paths first.")
    }
}

#[cfg(test)]
mod tests {
    use crate::path::{PathComponent, PathComponentBox, PathRc};

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
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            timestamp: 500,
            payload_length: 10,
            payload_digest: FakePayloadDigest::default(),
        };

        let range_including = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'b']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(range_including.includes_entry(&entry));

        let range_subspaces_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(11), FakeSubspaceId(13))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'b']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_subspaces_excluding.includes_entry(&entry));

        let range_paths_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_paths_excluding.includes_entry(&entry));

        let range_times_excluding = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(9), FakeSubspaceId(11))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'b']).unwrap()]).unwrap(),
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
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_subspace.is_none());

        let empty_intersection_path = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'4']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'6']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_path.is_none());

        let empty_intersection_time = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'1']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(200, 300).unwrap(),
        });

        assert!(empty_intersection_time.is_none());

        let intersection = Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(0), FakeSubspaceId(3))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'0']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'6']).unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<FakeSubspaceId>::new_closed(FakeSubspaceId(2), FakeSubspaceId(4))
                .unwrap(),
            paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                PathRc::new(&[PathComponentBox::new(&[b'3']).unwrap()]).unwrap(),
                PathRc::new(&[PathComponentBox::new(&[b'9']).unwrap()]).unwrap(),
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
                paths: Range::<PathRc<MCL, MCC, MPL>>::new_closed(
                    PathRc::new(&[PathComponentBox::new(&[b'3']).unwrap()]).unwrap(),
                    PathRc::new(&[PathComponentBox::new(&[b'6']).unwrap()]).unwrap(),
                )
                .unwrap(),
                times: Range::<Timestamp>::new_closed(50, 100).unwrap(),
            }
        )
    }
}
