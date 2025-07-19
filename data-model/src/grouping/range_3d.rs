#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use ufotofu_codec::Encodable;
use ufotofu_codec_endian::U64BE;

use crate::{
    entry::{Entry, Timestamp},
    grouping::{area::Area, range::Range},
    parameters::SubspaceId,
    path::Path,
};

/// A three-dimensional range that includes every Entry included in all three of its ranges.
///
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#D3Range).
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range3d<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// A range of [`SubspaceId`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#SubspaceRange).
    subspaces: Range<S>,
    /// A range of [`Path`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#PathRange).
    paths: Range<Path<MCL, MCC, MPL>>,
    /// A range of [`Timestamp`]s.
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#TimeRange).
    times: Range<Timestamp>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Range3d<MCL, MCC, MPL, S> {
    /// Creates a new [`Range3d`].
    pub fn new(
        subspaces: Range<S>,
        paths: Range<Path<MCL, MCC, MPL>>,
        times: Range<Timestamp>,
    ) -> Self {
        Range3d {
            subspaces,
            paths,
            times,
        }
    }

    /// Returns a reference to the range of [`SubspaceId`]s.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#SubspaceRange).
    pub fn subspaces(&self) -> &Range<S> {
        &self.subspaces
    }

    /// Returns a reference to the range of [`Path`]s.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#PathRange).
    pub fn paths(&self) -> &Range<Path<MCL, MCC, MPL>> {
        &self.paths
    }

    /// Returns a reference to the range of [`Timestamp`]s.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#TimeRange).
    pub fn times(&self) -> &Range<Timestamp> {
        &self.times
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Range3d<MCL, MCC, MPL, S>
where
    S: Default,
{
    /// Creates a new [`Range3d`] that covers everything. Requires `S::default()` to be the least `S` to be correct.
    pub fn new_full() -> Self {
        Self::new(
            Default::default(),
            Range::new_open(Path::new_empty()),
            Default::default(),
        )
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Range3d<MCL, MCC, MPL, S>
where
    S: PartialOrd,
{
    /// Returns whether an [`Entry`] is [included](https://willowprotocol.org/specs/grouping-entries/index.html#d3_range_include) by this 3d range.
    pub fn includes_entry<N, PD>(&self, entry: &Entry<MCL, MCC, MPL, N, S, PD>) -> bool {
        self.subspaces.includes(entry.subspace_id())
            && self.paths.includes(entry.path())
            && self.times.includes(&entry.timestamp())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Range3d<MCL, MCC, MPL, S>
where
    S: Ord + Clone,
{
    /// Returns the intersection between this [`Range3d`] and another.
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

impl<const MCL: usize, const MCC: usize, const MPL: usize, S: Default> Default
    for Range3d<MCL, MCC, MPL, S>
{
    /// The default 3dRange, assuming that `S::default()` yields the default SubspaceId.
    ///
    /// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#default_3d_range)
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
    fn from(value: Area<MCL, MCC, MPL, S>) -> Self {
        let subspaces = match value.subspace() {
            super::AreaSubspace::Any => Range::new_open(S::default()),
            super::AreaSubspace::Id(id) => match id.successor() {
                Some(successor) => {
                    super::Range::new(id.clone(), super::RangeEnd::Closed(successor))
                }
                None => super::Range::new_open(id.clone()),
            },
        };

        let paths = match value.path().successor() {
            Some(succesor) => {
                super::Range::new(value.path().clone(), super::RangeEnd::Closed(succesor))
            }
            None => super::Range::new_open(value.path().clone()),
        };

        Self {
            subspaces,
            paths,
            times: *value.times(),
        }
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, S> Arbitrary<'a>
    for Range3d<MCL, MCC, MPL, S>
where
    S: PartialOrd + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            subspaces: Arbitrary::arbitrary(u)?,
            paths: Arbitrary::arbitrary(u)?,
            times: Arbitrary::arbitrary(u)?,
        })
    }
}

/// This is an "inofficial" encoding not on the encodings spec page.
///
/// ## An Absolute Encoding Relation for 3dRange
///
/// - First byte is a header of bitflags:
///     - most significant bit: `1` iff the subspace range is open.
///     - second-most significant bit: `1` iff the path range is open.
///     - third-most significant bit: `1` iff the timestamp range is open.
///     - remaining five bits: arbitrary.
/// - encoding of the start of the subspace range
/// - encoding of the end of the subspace range, or empty string if the subspace range is open
/// - encoding of the start of the path range
/// - encoding of the end of the path range, or empty string if the path range is open
/// - encoding of the start of the timestamp range as an 8-byte big-endian integer
/// - encoding of the end of the timestamp range as an 8-byte big-endian integer, or empty string if the timestamp range is open
impl<const MCL: usize, const MCC: usize, const MPL: usize, S: Encodable> Encodable
    for Range3d<MCL, MCC, MPL, S>
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0;

        if self.subspaces().is_open() {
            header |= 0b1000_0000;
        }
        if self.paths().is_open() {
            header |= 0b0100_0000;
        }
        if self.times().is_open() {
            header |= 0b0010_0000;
        }

        consumer.consume(header).await?;

        self.subspaces().start.encode(consumer).await?;
        if let Some(end) = self.subspaces().get_end() {
            end.encode(consumer).await?;
        }

        self.paths().start.encode(consumer).await?;
        if let Some(end) = self.paths().get_end() {
            end.encode(consumer).await?;
        }

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
    fn includes_entry() {
        let entry = Entry::new(
            0,
            10,
            Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            500,
            10,
            0,
        );

        let range_including = Range3d {
            subspaces: Range::<u64>::new_closed(9, 11).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"b").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(range_including.includes_entry(&entry));

        let range_subspaces_excluding = Range3d {
            subspaces: Range::<u64>::new_closed(11, 13).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"b").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_subspaces_excluding.includes_entry(&entry));

        let range_paths_excluding = Range3d {
            subspaces: Range::<u64>::new_closed(9, 11).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(400, 600).unwrap(),
        };

        assert!(!range_paths_excluding.includes_entry(&entry));

        let range_times_excluding = Range3d {
            subspaces: Range::<u64>::new_closed(9, 11).unwrap(),
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
            subspaces: Range::<u64>::new_closed(11, 13).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_subspace.is_none());

        let empty_intersection_path = Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"4").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"6").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        });

        assert!(empty_intersection_path.is_none());

        let empty_intersection_time = Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"1").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(200, 300).unwrap(),
        });

        assert!(empty_intersection_time.is_none());

        let intersection = Range3d {
            subspaces: Range::<u64>::new_closed(0, 3).unwrap(),
            paths: Range::<Path<MCL, MCC, MPL>>::new_closed(
                Path::new_from_slice(&[Component::new(b"0").unwrap()]).unwrap(),
                Path::new_from_slice(&[Component::new(b"6").unwrap()]).unwrap(),
            )
            .unwrap(),
            times: Range::<Timestamp>::new_closed(0, 100).unwrap(),
        }
        .intersection(&Range3d {
            subspaces: Range::<u64>::new_closed(2, 4).unwrap(),
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
                subspaces: Range::<u64>::new_closed(2, 3).unwrap(),
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
