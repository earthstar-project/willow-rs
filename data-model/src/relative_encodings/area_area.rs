use compact_u64::{CompactU64, EncodingWidth, Tag, TagWidth};
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync, RelativeEncodable,
    RelativeEncodableKnownSize, RelativeEncodableSync,
};
use willow_encoding::is_bitflagged;

use crate::{
    decode_path_extends_path, decode_path_extends_path_canonic, encode_path_extends_path,
    grouping::{Area, AreaSubspace, Range, RangeEnd},
    path_extends_path_encoding_len, SubspaceId,
};

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodable<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + Encodable,
{
    /// Encodes this [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Area<MCL, MCC, MPL, S>,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        if !r.includes_area(self) {
            panic!("Tried to encode an area relative to a area it is not included by")
        }

        let start_diff = core::cmp::min(
            self.times().start - r.times().start,
            u64::from(&r.times().end) - self.times().start,
        );

        let end_diff = core::cmp::min(
            u64::from(&self.times().end) - r.times().start,
            u64::from(&r.times().end) - u64::from(&self.times().end),
        );

        let mut header = 0;

        if self.subspace() != r.subspace() {
            header |= 0b1000_0000;
        }

        if self.times().end == RangeEnd::Open {
            header |= 0b0100_0000;
        }

        if start_diff == self.times().start - r.times().start {
            header |= 0b0010_0000;
        }

        if self.times().end != RangeEnd::Open
            && end_diff == u64::from(&self.times().end) - r.times().start
        {
            header |= 0b0001_0000;
        }

        let start_diff_tag = Tag::min_tag(start_diff, TagWidth::two());
        let end_diff_tag = Tag::min_tag(end_diff, TagWidth::two());

        header |= start_diff_tag.data_at_offset(4);
        header |= end_diff_tag.data_at_offset(6);

        consumer.consume(header).await?;

        match (&self.subspace(), &r.subspace()) {
            (AreaSubspace::Any, AreaSubspace::Any) => {} // Same subspace
            (AreaSubspace::Id(_), AreaSubspace::Id(_)) => {} // Same subspace
            (AreaSubspace::Id(subspace), AreaSubspace::Any) => {
                subspace.encode(consumer).await?;
            }
            (AreaSubspace::Any, AreaSubspace::Id(_)) => {
                unreachable!(
                    "We should have already rejected an area not included by another area!"
                )
            }
        }

        CompactU64(start_diff)
            .relative_encode(consumer, &start_diff_tag.encoding_width())
            .await?;

        if self.times().end != RangeEnd::Open {
            CompactU64(end_diff)
                .relative_encode(consumer, &end_diff_tag.encoding_width())
                .await?;
        }

        encode_path_extends_path(consumer, self.path(), r.path()).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Area<MCL, MCC, MPL, S>, Blame> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    /// Decodes an [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Area<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<false, MCL, MCC, MPL, S, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableCanonic<Area<MCL, MCC, MPL, S>, Blame, Blame> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Area<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<true, MCL, MCC, MPL, S, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableKnownSize<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Area<MCL, MCC, MPL, S>) -> usize {
        if !r.includes_area(self) {
            panic!("Tried to encode an area relative to a area it is not included by")
        }

        let start_diff = core::cmp::min(
            self.times().start - r.times().start,
            u64::from(&r.times().end) - self.times().start,
        );

        let end_diff = core::cmp::min(
            u64::from(&self.times().end) - r.times().start,
            u64::from(&r.times().end) - u64::from(&self.times().end),
        );

        let start_diff_tag = Tag::min_tag(start_diff, TagWidth::two());
        let end_diff_tag = Tag::min_tag(end_diff, TagWidth::two());

        let subspace_len = match (&self.subspace(), &r.subspace()) {
            (AreaSubspace::Any, AreaSubspace::Any) => 0, // Same subspace
            (AreaSubspace::Id(_), AreaSubspace::Id(_)) => 0, // Same subspace
            (AreaSubspace::Id(subspace), AreaSubspace::Any) => subspace.len_of_encoding(),
            (AreaSubspace::Any, AreaSubspace::Id(_)) => {
                unreachable!(
                    "We should have already rejected an area not included by another area!"
                )
            }
        };

        let start_diff_len =
            CompactU64(start_diff).relative_len_of_encoding(&start_diff_tag.encoding_width());

        let end_diff_len = if self.times().end != RangeEnd::Open {
            CompactU64(end_diff).relative_len_of_encoding(&end_diff_tag.encoding_width())
        } else {
            0
        };

        let path_len = path_extends_path_encoding_len(self.path(), r.path());

        1 + subspace_len + path_len + start_diff_len + end_diff_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableSync<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableSync<Area<MCL, MCC, MPL, S>, Blame> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
}

async fn relative_decode_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S,
    P,
>(
    producer: &mut P,
    r: &Area<MCL, MCC, MPL, S>,
) -> Result<Area<MCL, MCC, MPL, S>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
    S: SubspaceId + DecodableCanonic,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    let header = producer.produce_item().await?;

    // Decode subspace?
    let is_subspace_encoded = is_bitflagged(header, 0);

    // Decode end value of times?
    let is_times_end_open = is_bitflagged(header, 1);

    // Add start_diff to out.get_times().start, or subtract from out.get_times().end?
    let add_start_diff = is_bitflagged(header, 2);

    // Add end_diff to out.get_times().start, or subtract from out.get_times().end?
    let add_end_diff = is_bitflagged(header, 3);

    // === Necessary to produce canonic encodings. ===
    // Verify that we don't add_end_diff when open...
    if CANONIC && add_end_diff && is_times_end_open {
        return Err(DecodeError::Other(Blame::TheirFault));
    }
    // ===============================================

    let start_time_diff_tag = Tag::from_raw(header, TagWidth::two(), 4);
    let end_time_diff_tag = Tag::from_raw(header, TagWidth::two(), 6);

    // === Necessary to produce canonic encodings. ===
    // Verify the last two bits are zero if is_times_end_open
    if CANONIC && is_times_end_open && (end_time_diff_tag.encoding_width() != EncodingWidth::one())
    {
        return Err(DecodeError::Other(Blame::TheirFault));
    }
    // ===============================================

    let subspace = if is_subspace_encoded {
        let id = if CANONIC {
            S::decode_canonic(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            S::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        };
        let sub = AreaSubspace::Id(id);

        // === Necessary to produce canonic encodings. ===
        // Verify that subspace wasn't needlessly encoded
        if CANONIC && &sub == r.subspace() {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
        // ===============================================

        sub
    } else {
        r.subspace().clone()
    };

    // Verify that the decoded subspace is included by the reference subspace
    match (&r.subspace(), &subspace) {
        (AreaSubspace::Any, AreaSubspace::Any) => {}
        (AreaSubspace::Any, AreaSubspace::Id(_)) => {}
        (AreaSubspace::Id(_), AreaSubspace::Any) => {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
        (AreaSubspace::Id(a), AreaSubspace::Id(b)) => {
            if a != b {
                return Err(DecodeError::Other(Blame::TheirFault));
            }
        }
    }

    let start_diff = if CANONIC {
        CompactU64::relative_decode_canonic(producer, &start_time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    } else {
        CompactU64::relative_decode(producer, &start_time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    };

    let start = if add_start_diff {
        r.times().start.checked_add(start_diff)
    } else {
        u64::from(&r.times().end).checked_sub(start_diff)
    }
    .ok_or(DecodeError::Other(Blame::TheirFault))?;

    // TODO: DOES THE BELOW NEED TO BE PART OF CANONIC CHECK?
    // Verify they sent correct start diff
    let expected_start_diff = core::cmp::min(
        start.checked_sub(r.times().start),
        u64::from(&r.times().end).checked_sub(start),
    )
    .ok_or(DecodeError::Other(Blame::TheirFault))?;

    if expected_start_diff != start_diff {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    if CANONIC {
        // === Necessary to produce canonic encodings. ===
        // Verify that bit 2 of the header was set correctly
        let should_add_start_diff = start_diff
            == start
                .checked_sub(r.times().start)
                .ok_or(DecodeError::Other(Blame::TheirFault))?;

        if add_start_diff != should_add_start_diff {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
        // ===============================================
    }

    let end = if is_times_end_open {
        if add_end_diff {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        RangeEnd::Open
    } else {
        let end_diff = if CANONIC {
            CompactU64::relative_decode_canonic(producer, &end_time_diff_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0
        } else {
            CompactU64::relative_decode(producer, &end_time_diff_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0
        };

        let end = if add_end_diff {
            r.times().start.checked_add(end_diff)
        } else {
            u64::from(&r.times().end).checked_sub(end_diff)
        }
        .ok_or(DecodeError::Other(Blame::TheirFault))?;

        // Verify they sent correct end diff
        let expected_end_diff = core::cmp::min(
            end.checked_sub(r.times().start),
            u64::from(&r.times().end).checked_sub(end),
        )
        .ok_or(DecodeError::Other(Blame::TheirFault))?;

        if end_diff != expected_end_diff {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        // === Necessary to produce canonic encodings. ===
        if CANONIC {
            let should_add_end_diff = end_diff
                == end
                    .checked_sub(r.times().start)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?;

            if add_end_diff != should_add_end_diff {
                return Err(DecodeError::Other(Blame::TheirFault));
            }
        }
        // ============================================

        RangeEnd::Closed(end)
    };

    let times = Range { start, end };

    // Verify the decoded time range is included by the reference time range
    if !r.times().includes_range(&times) {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let path = if CANONIC {
        decode_path_extends_path_canonic(producer, r.path())
            .await
            .map_err(DecodeError::map_other_from)?
    } else {
        decode_path_extends_path(producer, r.path())
            .await
            .map_err(DecodeError::map_other_from)?
    };

    // // Verify the decoded path is prefixed by the reference path
    // if !path.is_prefixed_by(r.path()) {
    //     return Err(DecodeError::Other(Blame::TheirFault));
    // }

    Ok(Area::new(subspace, path, times))
}
