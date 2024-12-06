// Entry <> Area

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, DecodeError, DecodingWentWrong, Encodable,
    EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use crate::{grouping::Range3d, Entry, NamespaceId, PayloadDigest, SubspaceId};

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

    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(N, Range3d<MCL, MCC, MPL, S>),
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        /*
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
        let add_time_diff_with_start = time_diff == self.timestamp().abs_diff(out.times().start);

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
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodable<(N, Range3d<MCL, MCC, MPL, S>), DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
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
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(N, Range3d<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
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

        let time_diff = decode_compact_width_be_relation(time_diff_compact_width, producer).await?;

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
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableCanonic<(N, Range3d<MCL, MCC, MPL, S>), DecodingWentWrong, DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableCanonic,
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    PD: PayloadDigest + DecodableCanonic,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &(N, Range3d<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
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
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableKnownSize<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &(N, Range3d<MCL, MCC, MPL, S>)) -> usize {
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableSync<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableSync,
    S: SubspaceId + EncodableSync,
    PD: PayloadDigest + EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableSync<(N, Range3d<MCL, MCC, MPL, S>), DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableSync,
    S: SubspaceId + DecodableSync + std::fmt::Debug,
    PD: PayloadDigest + DecodableSync,
{
}
