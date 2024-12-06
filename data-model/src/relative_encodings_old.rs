pub(super) mod encoding {
    use ufotofu::{BulkConsumer, BulkProducer};

    use ufotofu_codec::{
        Decodable, DecodableCanonic, DecodableSync, DecodeError, DecodingWentWrong, Encodable,
        EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
        RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize,
        RelativeEncodableSync,
    };

    use core::mem::size_of;

    use crate::{
        entry::Entry,
        grouping::{Area, AreaSubspace, Range, Range3d, RangeEnd},
        parameters::{NamespaceId, PayloadDigest, SubspaceId},
        path::Path,
        ScratchSpacePathDecoding,
    };

    impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
        RelativeEncodable<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
    where
        N: NamespaceId + Encodable,
        S: SubspaceId + Encodable,
        PD: PayloadDigest + Encodable,
    {
        /// Encodes this [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
        async fn relative_encode<Consumer>(
            &self,
            reference: &(N, Range3d<MCL, MCC, MPL, S>),
            consumer: &mut Consumer,
        ) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let (namespace, out) = reference;

            if self.namespace_id() != namespace {
                panic!("Tried to encode an entry relative to a namespace it does not belong to")
            }

            if !out.includes_entry(self) {
                panic!("Tried to encode an entry relative to a 3d range it is not included by")
            }

            let time_diff = core::cmp::min(
                self.timestamp().abs_diff(out.times().start),
                self.timestamp().abs_diff(u64::from(&out.times().end)),
            );

            let mut header = 0b0000_0000;

            // Encode e.get_subspace_id()?
            if self.subspace_id() != &out.subspaces().start {
                header |= 0b1000_0000;
            }

            // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
            let encode_path_relative_to_start = match &out.paths().end {
                RangeEnd::Closed(end_path) => {
                    let start_lcp = self.path().longest_common_prefix(&out.paths().start);
                    let end_lcp = self.path().longest_common_prefix(end_path);

                    start_lcp.component_count() >= end_lcp.component_count()
                }
                RangeEnd::Open => true,
            };

            if encode_path_relative_to_start {
                header |= 0b0100_0000;
            }

            // Add time_diff to out.get_times().start, or subtract from out.get_times().end?
            let add_time_diff_with_start =
                time_diff == self.timestamp().abs_diff(out.times().start);

            if add_time_diff_with_start {
                header |= 0b0010_0000;
            }

            // 2-bit integer n such that 2^n gives compact_width(time_diff)
            header |= CompactWidth::from_u64(time_diff).bitmask(4);

            // 2-bit integer n such that 2^n gives compact_width(e.get_payload_length())
            header |= CompactWidth::from_u64(self.payload_length()).bitmask(6);

            consumer.consume(header).await?;

            if self.subspace_id() != &out.subspaces().start {
                self.subspace_id().encode(consumer).await?;
            }

            // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
            match &out.paths().end {
                RangeEnd::Closed(end_path) => {
                    if encode_path_relative_to_start {
                        self.path()
                            .relative_encode(&out.paths().start, consumer)
                            .await?;
                    } else {
                        self.path().relative_encode(end_path, consumer).await?;
                    }
                }
                RangeEnd::Open => {
                    self.path()
                        .relative_encode(&out.paths().start, consumer)
                        .await?;
                }
            }

            encode_compact_width_be(time_diff, consumer).await?;
            encode_compact_width_be(self.payload_length(), consumer).await?;
            self.payload_digest().encode(consumer).await?;

            Ok(())
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
        RelativeDecodable<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
    where
        N: NamespaceId + Decodable,
        S: SubspaceId + Decodable + std::fmt::Debug,
        PD: PayloadDigest + Decodable,
    {
        /// Decodes an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
        ///
        /// Will return an error if the encoding has not been produced by the corresponding encoding function.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
        async fn relative_decode_canonical<Producer>(
            reference: &(N, Range3d<MCL, MCC, MPL, S>),
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let (namespace, out) = reference;

            let header = produce_byte(producer).await?;

            // Decode e.get_subspace_id()?
            let is_subspace_encoded = is_bitflagged(header, 0);

            // Decode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
            let decode_path_relative_to_start = is_bitflagged(header, 1);

            // Add time_diff to out.get_times().start, or subtract from out.get_times().end?
            let add_time_diff_with_start = is_bitflagged(header, 2);

            if is_bitflagged(header, 3) {
                return Err(DecodeError::InvalidInput);
            }

            let time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);
            let payload_length_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 6);

            let subspace_id = if is_subspace_encoded {
                S::decode_canonical(producer).await?
            } else {
                out.subspaces().start.clone()
            };

            // === Necessary to produce canonic encodings. ===
            // Verify that encoding the subspace was necessary.
            if subspace_id == out.subspaces().start && is_subspace_encoded {
                return Err(DecodeError::InvalidInput);
            }
            // ===============================================

            // Verify that subspace is included by range
            if !out.subspaces().includes(&subspace_id) {
                return Err(DecodeError::InvalidInput);
            }

            let path = if decode_path_relative_to_start {
                Path::relative_decode_canonical(&out.paths().start, producer).await?
            } else {
                match &out.paths().end {
                    RangeEnd::Closed(end_path) => {
                        Path::relative_decode_canonical(end_path, producer).await?
                    }
                    RangeEnd::Open => return Err(DecodeError::InvalidInput),
                }
            };

            // Verify that path is included by range
            if !out.paths().includes(&path) {
                return Err(DecodeError::InvalidInput);
            }

            // === Necessary to produce canonic encodings. ===
            // Verify that the path was encoded relative to the correct bound of the referenc path range.
            let should_have_encoded_path_relative_to_start = match &out.paths().end {
                RangeEnd::Closed(end_path) => {
                    let start_lcp = path.longest_common_prefix(&out.paths().start);
                    let end_lcp = path.longest_common_prefix(end_path);

                    start_lcp.component_count() >= end_lcp.component_count()
                }
                RangeEnd::Open => true,
            };

            if decode_path_relative_to_start != should_have_encoded_path_relative_to_start {
                return Err(DecodeError::InvalidInput);
            }
            // =================================================

            let time_diff = decode_compact_width_be(time_diff_compact_width, producer).await?;

            let payload_length =
                decode_compact_width_be(payload_length_compact_width, producer).await?;

            let payload_digest = PD::decode_canonical(producer).await?;

            let timestamp = if add_time_diff_with_start {
                out.times().start.checked_add(time_diff)
            } else {
                match &out.times().end {
                    RangeEnd::Closed(end_time) => end_time.checked_sub(time_diff),
                    RangeEnd::Open => u64::from(&out.times().end).checked_sub(time_diff),
                }
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Verify that timestamp is included by range
            if !out.times().includes(&timestamp) {
                return Err(DecodeError::InvalidInput);
            }

            // === Necessary to produce canonic encodings. ===
            // Verify that time_diff is what it should have been
            let correct_time_diff = core::cmp::min(
                timestamp.abs_diff(out.times().start),
                timestamp.abs_diff(u64::from(&out.times().end)),
            );

            if time_diff != correct_time_diff {
                return Err(DecodeError::InvalidInput);
            }

            // Verify that the combine with start bitflag in the header was correct
            let should_have_added_to_start = time_diff == timestamp.abs_diff(out.times().start);

            if should_have_added_to_start != add_time_diff_with_start {
                return Err(DecodeError::InvalidInput);
            }
            // ==============================================

            Ok(Self::new(
                namespace.clone(),
                subspace_id,
                path,
                timestamp,
                payload_length,
                payload_digest,
            ))
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
        RelativeRelationDecodable<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
    where
        N: NamespaceId + RelationDecodable,
        S: SubspaceId + RelationDecodable + std::fmt::Debug,
        PD: PayloadDigest + RelationDecodable,
    {
        /// Decodes an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
        async fn relative_decode_relation<Producer>(
            reference: &(N, Range3d<MCL, MCC, MPL, S>),
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let (namespace, out) = reference;

            let header = produce_byte(producer).await?;

            // Decode e.get_subspace_id()?
            let is_subspace_encoded = is_bitflagged(header, 0);

            // Decode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
            let decode_path_relative_to_start = is_bitflagged(header, 1);

            // Add time_diff to out.get_times().start, or subtract from out.get_times().end?
            let add_time_diff_with_start = is_bitflagged(header, 2);

            let time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);
            let payload_length_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 6);

            let subspace_id = if is_subspace_encoded {
                S::decode_relation(producer).await?
            } else {
                out.subspaces().start.clone()
            };

            // Verify that subspace is included by range
            if !out.subspaces().includes(&subspace_id) {
                return Err(DecodeError::InvalidInput);
            }

            let path = if decode_path_relative_to_start {
                Path::relative_decode_canonical(&out.paths().start, producer).await?
            } else {
                match &out.paths().end {
                    RangeEnd::Closed(end_path) => {
                        Path::relative_decode_canonical(end_path, producer).await?
                    }
                    RangeEnd::Open => return Err(DecodeError::InvalidInput),
                }
            };

            // Verify that path is included by range
            if !out.paths().includes(&path) {
                return Err(DecodeError::InvalidInput);
            }

            let time_diff =
                decode_compact_width_be_relation(time_diff_compact_width, producer).await?;

            let payload_length =
                decode_compact_width_be_relation(payload_length_compact_width, producer).await?;

            let payload_digest = PD::decode_relation(producer).await?;

            let timestamp = if add_time_diff_with_start {
                out.times().start.checked_add(time_diff)
            } else {
                match &out.times().end {
                    RangeEnd::Closed(end_time) => end_time.checked_sub(time_diff),
                    RangeEnd::Open => u64::from(&out.times().end).checked_sub(time_diff),
                }
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Verify that timestamp is included by range
            if !out.times().includes(&timestamp) {
                return Err(DecodeError::InvalidInput);
            }

            Ok(Self::new(
                namespace.clone(),
                subspace_id,
                path,
                timestamp,
                payload_length,
                payload_digest,
            ))
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeEncodable<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
    where
        S: SubspaceId + Encodable,
    {
        /// Encodes this [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_encode<Consumer>(
            &self,
            out: &Area<MCL, MCC, MPL, S>,
            consumer: &mut Consumer,
        ) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
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
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeDecodable<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
    where
        S: SubspaceId + Decodable,
    {
        /// Decodes an [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// Will return an error if the encoding has not been produced by the corresponding encoding function.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_decode_canonical<Producer>(
            out: &Area<MCL, MCC, MPL, S>,
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
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
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeRelationDecodable<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
    where
        S: SubspaceId + RelationDecodable,
    {
        /// Decodes an [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_decode_relation<Producer>(
            out: &Area<MCL, MCC, MPL, S>,
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
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
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeEncodable<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
    where
        S: SubspaceId + Encodable + std::fmt::Debug,
    {
        /// Encodes this [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_encode<Consumer>(
            &self,
            reference: &Range3d<MCL, MCC, MPL, S>,
            consumer: &mut Consumer,
        ) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let start_to_start = self.times().start.abs_diff(reference.times().start);
            let start_to_end = match reference.times().end {
                RangeEnd::Closed(end) => self.times().start.abs_diff(end),
                RangeEnd::Open => u64::MAX,
            };
            let end_to_start = match self.times().end {
                RangeEnd::Closed(end) => end.abs_diff(reference.times().start),
                RangeEnd::Open => u64::MAX,
            };
            let end_to_end = match (&self.times().end, &reference.times().end) {
                (RangeEnd::Closed(self_end), RangeEnd::Closed(ref_end)) => {
                    self_end.abs_diff(*ref_end)
                }
                (RangeEnd::Closed(_), RangeEnd::Open) => u64::MAX,
                (RangeEnd::Open, RangeEnd::Closed(_)) => u64::MAX,
                (RangeEnd::Open, RangeEnd::Open) => 0, // shouldn't matter right???
            };

            let start_time_diff = core::cmp::min(start_to_start, start_to_end);

            let end_time_diff = core::cmp::min(end_to_start, end_to_end);

            let mut header_1 = 0b0000_0000;

            // Bits 0, 1 - Encode r.get_subspaces().start?
            if self.subspaces().start == reference.subspaces().start {
                header_1 |= 0b0100_0000;
            } else if reference.subspaces().end == self.subspaces().start {
                header_1 |= 0b1000_0000;
            } else {
                header_1 |= 0b1100_0000;
            }

            // Bits 2, 3 - Encode r.get_subspaces().end?
            if self.subspaces().end == RangeEnd::Open {
                // Do nothing
            } else if self.subspaces().end == reference.subspaces().start {
                header_1 |= 0b0001_0000;
            } else if self.subspaces().end == reference.subspaces().end {
                header_1 |= 0b0010_0000;
            } else if self.subspaces().end != RangeEnd::Open {
                header_1 |= 0b0011_0000;
            }

            // Bit 4 - Encode r.get_paths().start relative to ref.get_paths().start or to ref.get_paths().end?
            if let RangeEnd::Closed(ref_path_end) = &reference.paths().end {
                let lcp_start_start = self
                    .paths()
                    .start
                    .longest_common_prefix(&reference.paths().start);
                let lcp_start_end = self.paths().start.longest_common_prefix(ref_path_end);

                if lcp_start_start.component_count() >= lcp_start_end.component_count() {
                    header_1 |= 0b0000_1000;
                }
            } else {
                header_1 |= 0b0000_1000;
            }

            // Bit 5 - Self path end open?
            if self.paths().end == RangeEnd::Open {
                header_1 |= 0b0000_0100;
            }

            // Bit 6 - Encode r.get_paths().end relative to ref.get_paths().start or to ref.get_paths().end (if at all)?
            match (&self.paths().end, &reference.paths().end) {
                (RangeEnd::Closed(self_path_end), RangeEnd::Closed(ref_path_end)) => {
                    let lcp_end_start =
                        self_path_end.longest_common_prefix(&reference.paths().start);
                    let lcp_end_end = self_path_end.longest_common_prefix(ref_path_end);

                    if lcp_end_start.component_count() >= lcp_end_end.component_count() {
                        header_1 |= 0b0000_0010;
                    }
                }
                (RangeEnd::Closed(_), RangeEnd::Open) => {
                    header_1 |= 0b0000_0010;
                }
                (RangeEnd::Open, RangeEnd::Closed(_)) => {}
                (RangeEnd::Open, RangeEnd::Open) => {}
            }

            // Bit 7 - Self time end open?
            if self.times().end == RangeEnd::Open {
                header_1 |= 0b0000_0001;
            }

            consumer.consume(header_1).await?;

            let mut header_2 = 0b0000_0000;

            // Bit 8 - Encode r.get_times().start relative to ref.get_times().start or ref.get_times().end?
            if start_to_start <= start_to_end {
                header_2 |= 0b1000_0000;
            }

            // Bit 9 -Add or subtract start_time_diff?
            if is_bitflagged(header_2, 0) && self.times().start >= reference.times().start
                || !is_bitflagged(header_2, 0) && self.times().start >= reference.times().end
            {
                header_2 |= 0b0100_0000;
            }

            // Bit 10, 11 - 2-bit integer n such that 2^n gives compact_width(start_time_diff)
            header_2 |= CompactWidth::from_u64(start_time_diff).bitmask(2);

            // Bit 12 - Encode r.get_times().end relative to ref.get_times().start or ref.get_times().end (if at all)?
            if self.times().end != RangeEnd::Open && end_to_start <= end_to_end {
                header_2 |= 0b0000_1000;
            }

            // Bit 13 - Add or subtract end_time_diff (if encoding it at all)?
            if self.times().end == RangeEnd::Open {
                // do nothing
            } else if (is_bitflagged(header_2, 4) && self.times().end >= reference.times().start)
                || (!is_bitflagged(header_2, 4) && self.times().end >= reference.times().end)
            {
                header_2 |= 0b0000_0100;
            }

            // Bits 14, 15 - ignored, or 2-bit integer n such that 2^n gives compact_width(end_time_diff)
            if self.times().end == RangeEnd::Open {
                // do nothing
            } else {
                header_2 |= CompactWidth::from_u64(end_time_diff).bitmask(6);
            }

            consumer.consume(header_2).await?;

            if (self.subspaces().start == reference.subspaces().start)
                || (reference.subspaces().end == self.subspaces().start)
            {
                // Don't encode
            } else {
                self.subspaces().start.encode(consumer).await?;
            }

            if self.subspaces().end == RangeEnd::Open
                || (self.subspaces().end == reference.subspaces().start)
                || (self.subspaces().end == reference.subspaces().end)
            {
                // Don't encode end subspace
            } else if let RangeEnd::Closed(end_subspace) = &self.subspaces().end {
                end_subspace.encode(consumer).await?;
            }

            if is_bitflagged(header_1, 4) {
                self.paths()
                    .start
                    .relative_encode(&reference.paths().start, consumer)
                    .await?;
            } else if let RangeEnd::Closed(end_path) = &reference.paths().end {
                self.paths()
                    .start
                    .relative_encode(end_path, consumer)
                    .await?;
            }

            if let RangeEnd::Closed(end_path) = &self.paths().end {
                if is_bitflagged(header_1, 6) {
                    end_path
                        .relative_encode(&reference.paths().start, consumer)
                        .await?
                } else if let RangeEnd::Closed(ref_end_path) = &reference.paths().end {
                    end_path.relative_encode(ref_end_path, consumer).await?;
                }
            }

            encode_compact_width_be(start_time_diff, consumer).await?;

            if self.times().end != RangeEnd::Open {
                encode_compact_width_be(end_time_diff, consumer).await?;
            }

            Ok(())
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeDecodable<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
    where
        S: SubspaceId + Decodable + std::fmt::Debug,
    {
        /// Decodes a [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// Will return an error if the encoding has not been produced by the corresponding encoding function.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_decode_canonical<Producer>(
            reference: &Range3d<MCL, MCC, MPL, S>,
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let header_1 = produce_byte(producer).await?;

            let subspace_start_flags = header_1 & 0b1100_0000;
            let subspace_end_flags = header_1 & 0b0011_0000;
            let is_path_start_rel_to_start = is_bitflagged(header_1, 4);
            let is_path_end_open = is_bitflagged(header_1, 5);
            let is_path_end_rel_to_start = is_bitflagged(header_1, 6);
            let is_times_end_open = is_bitflagged(header_1, 7);

            let header_2 = produce_byte(producer).await?;

            let is_time_start_rel_to_start = is_bitflagged(header_2, 0);
            let add_or_subtract_start_time_diff = is_bitflagged(header_2, 1);
            let start_time_diff_compact_width =
                CompactWidth::decode_fixed_width_bitmask(header_2, 2);
            let is_time_end_rel_to_start = is_bitflagged(header_2, 4);
            let add_or_subtract_end_time_diff = is_bitflagged(header_2, 5);
            let end_time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header_2, 6);

            // Decode subspace start
            let subspace_start = match subspace_start_flags {
                0b0100_0000 => reference.subspaces().start.clone(),
                0b1000_0000 => match &reference.subspaces().end {
                    RangeEnd::Closed(end) => end.clone(),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                0b1100_0000 => {
                    let decoded_subspace = S::decode_canonical(producer).await?;

                    if decoded_subspace == reference.subspaces().start
                        || reference.subspaces().end == decoded_subspace
                    {
                        return Err(DecodeError::InvalidInput);
                    }

                    decoded_subspace
                }
                // This can only be b0000_0000 (which is not valid!)
                _ => Err(DecodeError::InvalidInput)?,
            };

            let subspace_end = match subspace_end_flags {
                0b0000_0000 => RangeEnd::Open,
                0b0001_0000 => RangeEnd::Closed(reference.subspaces().start.clone()),
                0b0010_0000 => match &reference.subspaces().end {
                    RangeEnd::Closed(end) => RangeEnd::Closed(end.clone()),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                // This can only be 0b0011_0000
                _ => {
                    let decoded_subspace = RangeEnd::Closed(S::decode_canonical(producer).await?);

                    if decoded_subspace == reference.subspaces().start
                        || reference.subspaces().end == decoded_subspace
                    {
                        return Err(DecodeError::InvalidInput);
                    }

                    decoded_subspace
                }
            };

            // Check subspace end...

            let path_start = match (is_path_start_rel_to_start, &reference.paths().end) {
                (true, RangeEnd::Closed(_)) => {
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?
                }
                (true, RangeEnd::Open) => {
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?
                }
                (false, RangeEnd::Closed(path_end)) => {
                    Path::relative_decode_canonical(path_end, producer).await?
                }
                (false, RangeEnd::Open) => Err(DecodeError::InvalidInput)?,
            };

            // Canonicity check for path start
            match &reference.paths().end {
                RangeEnd::Closed(ref_path_end) => {
                    let lcp_start_start =
                        path_start.longest_common_prefix(&reference.paths().start);
                    let lcp_start_end = path_start.longest_common_prefix(ref_path_end);

                    let expected_is_start_rel_to_start =
                        lcp_start_start.component_count() >= lcp_start_end.component_count();

                    if expected_is_start_rel_to_start != is_path_start_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }
                }
                RangeEnd::Open => {
                    if !is_path_start_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }
                }
            }
            // Canonicity check for path start over

            let path_end = if is_path_end_open {
                RangeEnd::Open
            } else if is_path_end_rel_to_start {
                RangeEnd::Closed(
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?,
                )
            } else {
                match &reference.paths().end {
                    RangeEnd::Closed(end) => {
                        RangeEnd::Closed(Path::relative_decode_canonical(end, producer).await?)
                    }
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                }
            };

            // Canonicity check for path end
            match &path_end {
                RangeEnd::Closed(p_end) => match &reference.paths().end {
                    RangeEnd::Closed(ref_end) => {
                        let lcp_end_start = p_end.longest_common_prefix(&reference.paths().start);
                        let lcp_end_end = p_end.longest_common_prefix(ref_end);

                        let expected_is_path_end_rel_to_start =
                            lcp_end_start.component_count() >= lcp_end_end.component_count();

                        if expected_is_path_end_rel_to_start != is_path_end_rel_to_start {
                            return Err(DecodeError::InvalidInput);
                        }
                    }
                    RangeEnd::Open => {}
                },
                RangeEnd::Open => {
                    if is_path_end_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }
                }
            }
            // End canonicity check

            let start_time_diff =
                decode_compact_width_be(start_time_diff_compact_width, producer).await?;

            let time_start = match (is_time_start_rel_to_start, add_or_subtract_start_time_diff) {
                (true, true) => reference.times().start.checked_add(start_time_diff),
                (true, false) => reference.times().start.checked_sub(start_time_diff),
                (false, true) => match reference.times().end {
                    RangeEnd::Closed(ref_end) => ref_end.checked_add(start_time_diff),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                (false, false) => match reference.times().end {
                    RangeEnd::Closed(ref_end) => ref_end.checked_sub(start_time_diff),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Canonicity check for start time
            match reference.times().end {
                RangeEnd::Closed(ref_time_end) => {
                    let start_to_start = time_start.abs_diff(reference.times().start);
                    let start_to_end = time_start.abs_diff(ref_time_end);

                    let expected_is_start_rel_to_start = start_to_start <= start_to_end;

                    if expected_is_start_rel_to_start != is_time_start_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }

                    let expected_add_or_subtract_start_time_diff = is_time_start_rel_to_start
                        && time_start >= reference.times().start
                        || !expected_is_start_rel_to_start && time_start >= ref_time_end;

                    if expected_add_or_subtract_start_time_diff != add_or_subtract_start_time_diff {
                        return Err(DecodeError::InvalidInput);
                    }
                }
                RangeEnd::Open => {
                    if !is_time_start_rel_to_start {
                        return Err(DecodeError::InvalidInput);
                    }

                    // I need to very add or subtract time diff here.
                    let expected_add_or_subtract_time_diff = time_start >= reference.times().start;

                    if expected_add_or_subtract_time_diff != add_or_subtract_start_time_diff {
                        return Err(DecodeError::InvalidInput);
                    }
                }
            }
            // End of canonicity check for start time

            let time_end = if is_times_end_open {
                if add_or_subtract_end_time_diff {
                    return Err(DecodeError::InvalidInput);
                }

                if is_time_end_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }

                let end_time_diff_compact_width_flags = 0b0000_0011;
                if header_2 & end_time_diff_compact_width_flags != 0b0000_0000 {
                    return Err(DecodeError::InvalidInput);
                }

                RangeEnd::Open
            } else {
                let end_time_diff =
                    decode_compact_width_be(end_time_diff_compact_width, producer).await?;

                let time_end = match (is_time_end_rel_to_start, add_or_subtract_end_time_diff) {
                    (true, true) => reference
                        .times()
                        .start
                        .checked_add(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,

                    (true, false) => reference
                        .times()
                        .start
                        .checked_sub(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,

                    (false, true) => match reference.times().end {
                        RangeEnd::Closed(ref_end) => ref_end
                            .checked_add(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,

                        RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                    },
                    (false, false) => match reference.times().end {
                        RangeEnd::Closed(ref_end) => ref_end
                            .checked_sub(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,

                        RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                    },
                };

                let end_to_start = time_end.abs_diff(reference.times().start);
                let end_to_end = match &reference.times().end {
                    RangeEnd::Closed(ref_end) => time_end.abs_diff(*ref_end),
                    RangeEnd::Open => u64::MAX,
                };

                let expected_is_time_end_rel_to_start = end_to_start <= end_to_end;
                if expected_is_time_end_rel_to_start != is_time_end_rel_to_start {
                    return Err(DecodeError::InvalidInput);
                }

                let expected_end_time_diff = core::cmp::min(end_to_start, end_to_end);

                if expected_end_time_diff != end_time_diff {
                    return Err(DecodeError::InvalidInput);
                }

                let expected_add_or_subtract_end_time_diff = (is_time_end_rel_to_start
                    && time_end >= reference.times().start)
                    || (!is_time_end_rel_to_start && time_end >= reference.times().end);

                if expected_add_or_subtract_end_time_diff != add_or_subtract_end_time_diff {
                    return Err(DecodeError::InvalidInput);
                }

                RangeEnd::Closed(time_end)
            };

            Ok(Self::new(
                Range {
                    start: subspace_start,
                    end: subspace_end,
                },
                Range {
                    start: path_start,
                    end: path_end,
                },
                Range {
                    start: time_start,
                    end: time_end,
                },
            ))
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
        RelativeRelationDecodable<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
    where
        S: SubspaceId + RelationDecodable + std::fmt::Debug,
    {
        /// Decodes a [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
        ///
        /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
        async fn relative_decode_relation<Producer>(
            reference: &Range3d<MCL, MCC, MPL, S>,
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let header_1 = produce_byte(producer).await?;

            let subspace_start_flags = header_1 & 0b1100_0000;
            let subspace_end_flags = header_1 & 0b0011_0000;
            let is_path_start_rel_to_start = is_bitflagged(header_1, 4);
            let is_path_end_open = is_bitflagged(header_1, 5);
            let is_path_end_rel_to_start = is_bitflagged(header_1, 6);
            let is_times_end_open = is_bitflagged(header_1, 7);

            let header_2 = produce_byte(producer).await?;

            let is_time_start_rel_to_start = is_bitflagged(header_2, 0);
            let add_or_subtract_start_time_diff = is_bitflagged(header_2, 1);
            let start_time_diff_compact_width =
                CompactWidth::decode_fixed_width_bitmask(header_2, 2);
            let is_time_end_rel_to_start = is_bitflagged(header_2, 4);
            let add_or_subtract_end_time_diff = is_bitflagged(header_2, 5);
            let end_time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header_2, 6);

            // Decode subspace start
            let subspace_start = match subspace_start_flags {
                0b0100_0000 => reference.subspaces().start.clone(),
                0b1000_0000 => match &reference.subspaces().end {
                    RangeEnd::Closed(end) => end.clone(),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                0b1100_0000 => S::decode_canonical(producer).await?,
                // This can only be b0000_0000 (which is not valid!)
                _ => Err(DecodeError::InvalidInput)?,
            };

            let subspace_end = match subspace_end_flags {
                0b0000_0000 => RangeEnd::Open,
                0b0001_0000 => RangeEnd::Closed(reference.subspaces().start.clone()),
                0b0010_0000 => match &reference.subspaces().end {
                    RangeEnd::Closed(end) => RangeEnd::Closed(end.clone()),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                // This can only be 0b0011_0000
                _ => RangeEnd::Closed(S::decode_relation(producer).await?),
            };

            let path_start = match (is_path_start_rel_to_start, &reference.paths().end) {
                (true, RangeEnd::Closed(_)) => {
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?
                }
                (true, RangeEnd::Open) => {
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?
                }
                (false, RangeEnd::Closed(path_end)) => {
                    Path::relative_decode_canonical(path_end, producer).await?
                }
                (false, RangeEnd::Open) => Err(DecodeError::InvalidInput)?,
            };

            let path_end = if is_path_end_open {
                RangeEnd::Open
            } else if is_path_end_rel_to_start {
                RangeEnd::Closed(
                    Path::relative_decode_canonical(&reference.paths().start, producer).await?,
                )
            } else {
                match &reference.paths().end {
                    RangeEnd::Closed(end) => {
                        RangeEnd::Closed(Path::relative_decode_canonical(end, producer).await?)
                    }
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                }
            };

            let start_time_diff =
                decode_compact_width_be_relation(start_time_diff_compact_width, producer).await?;

            let time_start = match (is_time_start_rel_to_start, add_or_subtract_start_time_diff) {
                (true, true) => reference.times().start.checked_add(start_time_diff),
                (true, false) => reference.times().start.checked_sub(start_time_diff),
                (false, true) => match reference.times().end {
                    RangeEnd::Closed(ref_end) => ref_end.checked_add(start_time_diff),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                (false, false) => match reference.times().end {
                    RangeEnd::Closed(ref_end) => ref_end.checked_sub(start_time_diff),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
            }
            .ok_or(DecodeError::InvalidInput)?;

            let time_end = if is_times_end_open {
                RangeEnd::Open
            } else {
                let end_time_diff =
                    decode_compact_width_be_relation(end_time_diff_compact_width, producer).await?;

                let time_end = match (is_time_end_rel_to_start, add_or_subtract_end_time_diff) {
                    (true, true) => reference
                        .times()
                        .start
                        .checked_add(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,

                    (true, false) => reference
                        .times()
                        .start
                        .checked_sub(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,

                    (false, true) => match reference.times().end {
                        RangeEnd::Closed(ref_end) => ref_end
                            .checked_add(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,

                        RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                    },
                    (false, false) => match reference.times().end {
                        RangeEnd::Closed(ref_end) => ref_end
                            .checked_sub(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,

                        RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                    },
                };

                RangeEnd::Closed(time_end)
            };

            Ok(Self::new(
                Range {
                    start: subspace_start,
                    end: subspace_end,
                },
                Range {
                    start: path_start,
                    end: path_end,
                },
                Range {
                    start: time_start,
                    end: time_end,
                },
            ))
        }
    }
}
