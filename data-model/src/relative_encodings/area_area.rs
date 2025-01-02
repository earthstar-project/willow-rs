use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, DecodeError, Blame, Encodable,
    EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use crate::{grouping::Area, SubspaceId};

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
        /*
        if !out.includes_area(self) {
            panic!("Tried to encode an area relative to a area it is not included by")
        }

        let start_diff = core::cmp::min(
            self.times().start - out.times().start,
            u64::from(&out.times().end) - self.times().start,
        );

        let end_diff = core::cmp::min(
            u64::from(&self.times().end) - out.times().start,
            u64::from(&out.times().end) - u64::from(&self.times().end),
        );

        let mut header = 0;

        if self.subspace() != out.subspace() {
            header |= 0b1000_0000;
        }

        if self.times().end == RangeEnd::Open {
            header |= 0b0100_0000;
        }

        if start_diff == self.times().start - out.times().start {
            header |= 0b0010_0000;
        }

        if self.times().end != RangeEnd::Open
            && end_diff == u64::from(&self.times().end) - out.times().start
        {
            header |= 0b0001_0000;
        }

        header |= CompactWidth::from_u64(start_diff).bitmask(4);
        header |= CompactWidth::from_u64(end_diff).bitmask(6);

        consumer.consume(header).await?;

        match (&self.subspace(), &out.subspace()) {
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

        self.path().relative_encode(out.path(), consumer).await?;

        encode_compact_width_be(start_diff, consumer).await?;

        if self.times().end != RangeEnd::Open {
            encode_compact_width_be(end_diff, consumer).await?;
        }

        Ok(())
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Area<MCL, MCC, MPL, S>, Blame> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + Decodable,
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
        /*
        let header = produce_byte(producer).await?;

        // Decode subspace?
        let is_subspace_encoded = is_bitflagged(header, 0);

        // Decode end value of times?
        let is_times_end_open = is_bitflagged(header, 1);

        // Add start_diff to out.get_times().start, or subtract from out.get_times().end?
        let add_start_diff = is_bitflagged(header, 2);

        // Add end_diff to out.get_times().start, or subtract from out.get_times().end?
        let add_end_diff = is_bitflagged(header, 3);

        let start_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);
        let end_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 6);

        let subspace = if is_subspace_encoded {
            let id = S::decode_relation(producer).await?;
            AreaSubspace::Id(id)
        } else {
            out.subspace().clone()
        };

        // Verify that the decoded subspace is included by the reference subspace
        match (&out.subspace(), &subspace) {
            (AreaSubspace::Any, AreaSubspace::Any) => {}
            (AreaSubspace::Any, AreaSubspace::Id(_)) => {}
            (AreaSubspace::Id(_), AreaSubspace::Any) => {
                return Err(DecodeError::InvalidInput);
            }
            (AreaSubspace::Id(a), AreaSubspace::Id(b)) => {
                if a != b {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }

        let path = Path::relative_decode_canonical(out.path(), producer).await?;

        // Verify the decoded path is prefixed by the reference path
        if !path.is_prefixed_by(out.path()) {
            return Err(DecodeError::InvalidInput);
        }

        let start_diff =
            decode_compact_width_be_relation(start_diff_compact_width, producer).await?;

        let start = if add_start_diff {
            out.times().start.checked_add(start_diff)
        } else {
            u64::from(&out.times().end).checked_sub(start_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify they sent correct start diff
        let expected_start_diff = core::cmp::min(
            start.checked_sub(out.times().start),
            u64::from(&out.times().end).checked_sub(start),
        )
        .ok_or(DecodeError::InvalidInput)?;

        if expected_start_diff != start_diff {
            return Err(DecodeError::InvalidInput);
        }

        let end = if is_times_end_open {
            if add_end_diff {
                return Err(DecodeError::InvalidInput);
            }

            RangeEnd::Open
        } else {
            let end_diff =
                decode_compact_width_be_relation(end_diff_compact_width, producer).await?;

            let end = if add_end_diff {
                out.times().start.checked_add(end_diff)
            } else {
                u64::from(&out.times().end).checked_sub(end_diff)
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Verify they sent correct end diff
            let expected_end_diff = core::cmp::min(
                end.checked_sub(out.times().start),
                u64::from(&out.times().end).checked_sub(end),
            )
            .ok_or(DecodeError::InvalidInput)?;

            if end_diff != expected_end_diff {
                return Err(DecodeError::InvalidInput);
            }

            RangeEnd::Closed(end)
        };

        let times = Range { start, end };

        // Verify the decoded time range is included by the reference time range
        if !out.times().includes_range(&times) {
            return Err(DecodeError::InvalidInput);
        }

        Ok(Self::new(subspace, path, times))
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodableCanonic<Area<MCL, MCC, MPL, S>, Blame, Blame>
    for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Area<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
        let header = produce_byte(producer).await?;

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
        if add_end_diff && is_times_end_open {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        let start_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);
        let end_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 6);

        // === Necessary to produce canonic encodings. ===
        // Verify the last two bits are zero if is_times_end_open
        if is_times_end_open && (end_diff_compact_width != CompactWidth::One) {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        let subspace = if is_subspace_encoded {
            let id = S::decode_canonical(producer).await?;
            let sub = AreaSubspace::Id(id);

            // === Necessary to produce canonic encodings. ===
            // Verify that subspace wasn't needlessly encoded
            if &sub == out.subspace() {
                return Err(DecodeError::InvalidInput);
            }
            // ===============================================

            sub
        } else {
            out.subspace().clone()
        };

        // Verify that the decoded subspace is included by the reference subspace
        match (&out.subspace(), &subspace) {
            (AreaSubspace::Any, AreaSubspace::Any) => {}
            (AreaSubspace::Any, AreaSubspace::Id(_)) => {}
            (AreaSubspace::Id(_), AreaSubspace::Any) => {
                return Err(DecodeError::InvalidInput);
            }
            (AreaSubspace::Id(a), AreaSubspace::Id(b)) => {
                if a != b {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }

        let path = Path::relative_decode_canonical(out.path(), producer).await?;

        // Verify the decoded path is prefixed by the reference path
        if !path.is_prefixed_by(out.path()) {
            return Err(DecodeError::InvalidInput);
        }

        let start_diff = decode_compact_width_be(start_diff_compact_width, producer).await?;

        let start = if add_start_diff {
            out.times().start.checked_add(start_diff)
        } else {
            u64::from(&out.times().end).checked_sub(start_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify they sent correct start diff
        let expected_start_diff = core::cmp::min(
            start.checked_sub(out.times().start),
            u64::from(&out.times().end).checked_sub(start),
        )
        .ok_or(DecodeError::InvalidInput)?;

        if expected_start_diff != start_diff {
            return Err(DecodeError::InvalidInput);
        }

        // === Necessary to produce canonic encodings. ===
        // Verify that bit 2 of the header was set correctly
        let should_add_start_diff = start_diff
            == start
                .checked_sub(out.times().start)
                .ok_or(DecodeError::InvalidInput)?;

        if add_start_diff != should_add_start_diff {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        let end = if is_times_end_open {
            if add_end_diff {
                return Err(DecodeError::InvalidInput);
            }

            RangeEnd::Open
        } else {
            let end_diff = decode_compact_width_be(end_diff_compact_width, producer).await?;

            let end = if add_end_diff {
                out.times().start.checked_add(end_diff)
            } else {
                u64::from(&out.times().end).checked_sub(end_diff)
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Verify they sent correct end diff
            let expected_end_diff = core::cmp::min(
                end.checked_sub(out.times().start),
                u64::from(&out.times().end).checked_sub(end),
            )
            .ok_or(DecodeError::InvalidInput)?;

            if end_diff != expected_end_diff {
                return Err(DecodeError::InvalidInput);
            }

            // === Necessary to produce canonic encodings. ===
            let should_add_end_diff = end_diff
                == end
                    .checked_sub(out.times().start)
                    .ok_or(DecodeError::InvalidInput)?;

            if add_end_diff != should_add_end_diff {
                return Err(DecodeError::InvalidInput);
            }
            // ============================================

            RangeEnd::Closed(end)
        };

        let times = Range { start, end };

        // Verify the decoded time range is included by the reference time range
        if !out.times().includes_range(&times) {
            return Err(DecodeError::InvalidInput);
        }

        Ok(Self::new(subspace, path, times))
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableKnownSize<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Area<MCL, MCC, MPL, S>) -> usize {
        todo!()
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
    S: SubspaceId + DecodableSync,
{
}
