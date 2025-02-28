use arbitrary::Arbitrary;
use compact_u64::{CompactU64, Tag, TagWidth};
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
};
use willow_encoding::is_bitflagged;

use crate::{
    grouping::{Area, AreaSubspace, Range, RangeEnd},
    Entry, NamespaceId, Path, PayloadDigest, PrivatePathContext, SubspaceId,
};

#[derive(Debug, Clone)]
/// Confidential data that relates to determining the AreasOfInterest that peers might be interested in synchronising.
// TODO: Move this all to WGPS?
pub struct PrivateInterest<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
> {
    namespace_id: N,
    subspace_id: AreaSubspace<S>,
    path: Path<MCL, MCC, MPL>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    PrivateInterest<MCL, MCC, MPL, N, S>
{
    pub fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    pub fn subspace_id(&self) -> &AreaSubspace<S> {
        &self.subspace_id
    }

    pub fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.path
    }

    pub fn is_more_specific(&self, other: &Self) -> bool {
        self.namespace_id == other.namespace_id
            && other.subspace_id.includes_area_subspace(&self.subspace_id)
            && self.path.is_prefixed_by(&other.path)
    }

    pub fn is_less_specific(&self, other: &Self) -> bool {
        other.is_more_specific(self)
    }

    pub fn is_comparable(&self, other: &Self) -> bool {
        self.is_more_specific(other) || other.is_more_specific(other)
    }

    pub fn includes_entry<PD: PayloadDigest>(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool {
        &self.namespace_id == entry.namespace_id()
            && self.subspace_id.includes(entry.subspace_id())
            && self.path.is_prefix_of(entry.path())
    }

    pub fn is_disjoint(&self, other: &Self) -> bool {
        // We say that p1 and p2 are disjoint there can be no Entry which is included in both p1 and p2.

        if self.namespace_id != other.namespace_id {
            return true;
        }

        if !self.subspace_id.includes_area_subspace(&other.subspace_id)
            && !self.subspace_id.includes_area_subspace(&other.subspace_id)
        {
            return true;
        }

        if !self.path.is_related(&other.path) {
            return true;
        }

        false
    }

    pub fn awkward(&self, other: &Self) -> bool {
        // We say that p1 and p2 are awkward if they are neither comparable nor disjoint. This is the case if and only if one of them has subspace_id any and a path p, and the other has a non-any subspace_id and a path which is a strict prefix of p.
        !self.is_comparable(other) && !self.is_disjoint(other)
    }

    pub fn includes_area(&self, area: &Area<MCL, MCC, MPL, S>) -> bool {
        self.subspace_id.includes_area_subspace(area.subspace())
            && self.path.is_prefix_of(area.path())
    }

    pub fn is_related_to_area(&self, area: &Area<MCL, MCC, MPL, S>) -> bool {
        self.subspace_id.includes_area_subspace(area.subspace())
            && self.path.is_related(area.path())
    }

    pub fn almost_includes_area(&self, area: &Area<MCL, MCC, MPL, S>) -> bool {
        let subspace_is_fine = match (self.subspace_id(), area.subspace()) {
            (AreaSubspace::Id(self_id), AreaSubspace::Id(other_id)) => self_id == other_id,
            _ => true,
        };

        subspace_is_fine && self.path.is_related(area.path())
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N, S> Arbitrary<'a>
    for PrivateInterest<MCL, MCC, MPL, N, S>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_id: N = Arbitrary::arbitrary(u)?;
        let subspace_id: AreaSubspace<S> = Arbitrary::arbitrary(u)?;
        let path: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;

        Ok(Self {
            namespace_id,
            subspace_id,
            path,
        })
    }
}

#[derive(Debug)]
/// The context necessary to privately encode Areas.
pub struct PrivateAreaContext<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
> {
    /// The PrivateInterest to be kept private.
    private: PrivateInterest<MCL, MCC, MPL, N, S>,

    /// The almost containing Area relative to which we encode.
    rel: Area<MCL, MCC, MPL, S>,
}

#[derive(Debug)]
pub struct AreaNotAlmostIncludedError;

impl<const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    PrivateAreaContext<MCL, MCC, MPL, N, S>
{
    pub fn new(
        private: PrivateInterest<MCL, MCC, MPL, N, S>,
        rel: Area<MCL, MCC, MPL, S>,
    ) -> Result<Self, AreaNotAlmostIncludedError> {
        if !private.path.is_prefix_of(rel.path()) {
            // not almost included, wa
        }

        Ok(Self { private, rel })
    }

    pub fn private(&self) -> &PrivateInterest<MCL, MCC, MPL, N, S> {
        &self.private
    }

    pub fn rel(&self) -> &Area<MCL, MCC, MPL, S> {
        &self.rel
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    Arbitrary<'a> for PrivateAreaContext<MCL, MCC, MPL, N, S>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            private: Arbitrary::arbitrary(u)?,
            rel: Arbitrary::arbitrary(u)?,
        })
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    RelativeEncodable<PrivateAreaContext<MCL, MCC, MPL, N, S>> for Area<MCL, MCC, MPL, S>
where
    S: Encodable,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PrivateAreaContext<MCL, MCC, MPL, N, S>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if !r.rel.almost_includes_area(self) || !r.private.almost_includes_area(self) {
            panic!(
                "Tried to encode an Area relative to a PrivateAreaContext it is not almost included by."
            )
        }

        let (start_from_start, end_from_start) = match (r.rel.times().end, self.times().end) {
            (RangeEnd::Open, RangeEnd::Open) => (true, false),
            (RangeEnd::Open, RangeEnd::Closed(_)) => (true, true),
            (RangeEnd::Closed(_), RangeEnd::Open) => (false, false),
            (RangeEnd::Closed(rel_end), RangeEnd::Closed(val_end)) => {
                let end_from_start = val_end - r.rel().times().start <= rel_end - val_end;

                (false, end_from_start)
            }
        };

        let mut header = 0b0000_0000;

        if self.subspace() != r.rel().subspace() {
            header |= 0b1000_0000;
        }

        if self.subspace() == &AreaSubspace::Any {
            header |= 0b0100_0000
        }

        if start_from_start {
            header |= 0b0010_0000
        }

        if end_from_start {
            header |= 0b0001_0000
        }

        let start_diff = match (r.rel.times().end, self.times().end) {
            (RangeEnd::Open, RangeEnd::Open) => self.times().start - r.rel.times().start,
            (RangeEnd::Open, RangeEnd::Closed(_)) => self.times().start - r.rel.times().start,
            (RangeEnd::Closed(r_end), RangeEnd::Open) => r_end - self.times().start,
            (RangeEnd::Closed(r_end), RangeEnd::Closed(_)) => r_end - self.times().start,
        };

        let start_tag = Tag::min_tag(start_diff, TagWidth::two());
        header |= start_tag.data_at_offset(4);

        let end_diff = match (r.rel.times().end, self.times().end) {
            (RangeEnd::Open, RangeEnd::Open) | (RangeEnd::Closed(_), RangeEnd::Open) => None,
            (RangeEnd::Open, RangeEnd::Closed(val_end)) => {
                let diff = val_end - r.rel.times().start;

                let tag = Tag::min_tag(diff, TagWidth::two());
                header |= tag.data_at_offset(6);
                Some(diff)
            }
            (RangeEnd::Closed(rel_end), RangeEnd::Closed(val_end)) => {
                let end_from_start = val_end - r.rel().times().start <= rel_end - val_end;
                //   println!("end_from_start: {:?}", end_from_start);

                let diff = if end_from_start {
                    val_end - r.rel.times().start
                } else {
                    rel_end - val_end
                };

                let tag = Tag::min_tag(diff, TagWidth::two());
                header |= tag.data_at_offset(6);
                Some(diff)
            }
        };

        consumer.consume(header).await?;

        if let AreaSubspace::Id(id) = self.subspace() {
            if r.rel().subspace() != id {
                id.encode(consumer).await?;
            }
        }

        CompactU64(start_diff)
            .relative_encode(consumer, &start_tag.encoding_width())
            .await?;

        if let Some(end_diff) = end_diff {
            let end_from_start_tag = Tag::min_tag(end_diff, TagWidth::two());
            CompactU64(end_diff)
                .relative_encode(consumer, &end_from_start_tag.encoding_width())
                .await?;
        }

        // We can use new_unchecked because we know the paths from the context prefix the area's path.
        let private_path_context =
            PrivatePathContext::new_unchecked(r.private().path().clone(), r.rel().path().clone());

        self.path()
            .relative_encode(consumer, &private_path_context)
            .await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    RelativeDecodable<PrivateAreaContext<MCL, MCC, MPL, N, S>, Blame> for Area<MCL, MCC, MPL, S>
where
    S: Decodable,
    Blame: From<S::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PrivateAreaContext<MCL, MCC, MPL, N, S>,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let is_subspace_not_equal = is_bitflagged(header, 0);
        let is_subspace_any = is_bitflagged(header, 1);
        let is_start_from_start = is_bitflagged(header, 2);
        let is_end_from_start = is_bitflagged(header, 3);
        let start_diff_tag = Tag::from_raw(header, TagWidth::two(), 4);
        let end_diff_tag = Tag::from_raw(header, TagWidth::two(), 6);

        let subspace = if !is_subspace_any && is_subspace_not_equal {
            let id = S::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?;

            AreaSubspace::Id(id)
        } else if is_subspace_any {
            AreaSubspace::Any
        } else {
            r.rel().subspace().clone()
        };

        let start_diff = CompactU64::relative_decode(producer, &start_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?;

        let time_start = if is_start_from_start {
            start_diff
                .0
                .checked_add(r.rel().times().start)
                .ok_or(DecodeError::Other(Blame::TheirFault))?
        } else {
            match r.rel().times().end {
                RangeEnd::Closed(end) => end
                    .checked_sub(start_diff.0)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?,
                RangeEnd::Open => {
                    return Err(DecodeError::Other(Blame::TheirFault));
                }
            }
        };

        let is_time_end_open = is_start_from_start && !is_end_from_start;

        let time_end = if is_time_end_open {
            RangeEnd::Open
        } else {
            let end_diff = CompactU64::relative_decode(producer, &end_diff_tag)
                .await
                .map_err(DecodeError::map_other_from)?;

            let end = if is_end_from_start {
                //   println!("a");

                end_diff
                    .0
                    .checked_add(r.rel.times().start)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?
            } else {
                match r.rel.times().end {
                    RangeEnd::Closed(end) => {
                        //   println!("b");
                        end.checked_sub(end_diff.0)
                            .ok_or(DecodeError::Other(Blame::TheirFault))?

                        /*
                        end_diff
                            .0
                            .checked_add(end)
                            .ok_or(DecodeError::Other(Blame::TheirFault))?
                            */
                    }
                    RangeEnd::Open => {
                        //      println!("c");
                        return Err(DecodeError::Other(Blame::TheirFault));
                    }
                }
            };

            RangeEnd::Closed(end)
        };

        // We can use new_unchecked because we know the paths from the context prefix the area's path.
        let private_path_context =
            PrivatePathContext::new_unchecked(r.private().path().clone(), r.rel().path().clone());

        let path = Path::relative_decode(producer, &private_path_context).await?;

        if time_start > time_end {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        let times = Range::new(time_start, time_end);

        Ok(Area::new(subspace, path, times))
    }
}
