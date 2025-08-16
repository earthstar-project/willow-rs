// Entry <> Range3d

use compact_u64::{CompactU64, Tag, TagWidth};
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize,
    RelativeEncodableSync,
};
use willow_encoding::is_bitflagged;

use crate::{
    grouping::{Range3d, RangeEnd},
    Entry, NamespaceId, Path, PayloadDigest, SubspaceId,
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
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(N, Range3d<MCL, MCC, MPL, S>),
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = r;

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

        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::three());

        header |= time_diff_tag.data_at_offset(3);
        header |= payload_length_tag.data_at_offset(5);

        consumer.consume(header).await?;

        if self.subspace_id() != &out.subspaces().start {
            self.subspace_id().encode(consumer).await?;
        }

        // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
        match &out.paths().end {
            RangeEnd::Closed(end_path) => {
                if encode_path_relative_to_start {
                    self.path()
                        .relative_encode(consumer, &out.paths().start)
                        .await?;
                } else {
                    self.path().relative_encode(consumer, end_path).await?;
                }
            }
            RangeEnd::Open => {
                self.path()
                    .relative_encode(consumer, &out.paths().start)
                    .await?;
            }
        }

        CompactU64(time_diff)
            .relative_encode(consumer, &time_diff_tag.encoding_width())
            .await?;

        CompactU64(self.payload_length())
            .relative_encode(consumer, &payload_length_tag.encoding_width())
            .await?;

        self.payload_digest().encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodable<(N, Range3d<MCL, MCC, MPL, S>), Blame> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableCanonic,
    S: SubspaceId + DecodableCanonic,
    PD: PayloadDigest + DecodableCanonic,
    Blame: From<N::ErrorReason>
        + From<S::ErrorReason>
        + From<PD::ErrorReason>
        + From<N::ErrorCanonic>
        + From<S::ErrorCanonic>
        + From<PD::ErrorCanonic>,
{
    /// Decodes an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(N, Range3d<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let (namespace, out) = r;

        let header = producer.produce_item().await?;

        // Decode e.get_subspace_id()?
        let is_subspace_encoded = is_bitflagged(header, 0);

        // Decode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
        let decode_path_relative_to_start = is_bitflagged(header, 1);

        // Add time_diff to out.get_times().start, or subtract from out.get_times().end?
        let add_time_diff_to_start = is_bitflagged(header, 2);

        let time_diff_tag = Tag::from_raw(header, TagWidth::two(), 3);
        let payload_length_tag = Tag::from_raw(header, TagWidth::three(), 5);

        let subspace_id = if is_subspace_encoded {
            S::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            out.subspaces().start.clone()
        };

        // === Necessary to produce canonic encodings. ===
        // Verify that encoding the subspace was necessary.
        if subspace_id == out.subspaces().start && is_subspace_encoded {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
        // ===============================================

        // Verify that subspace is included by range
        if !out.subspaces().includes(&subspace_id) {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        let path = if decode_path_relative_to_start {
            Path::relative_decode(producer, &out.paths().start).await?
        } else {
            match &out.paths().end {
                RangeEnd::Closed(end_path) => Path::relative_decode(producer, end_path).await?,
                RangeEnd::Open => return Err(DecodeError::Other(Blame::TheirFault)),
            }
        };

        // Verify that path is included by range
        if !out.paths().includes(&path) {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        let time_diff = CompactU64::relative_decode(producer, &time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let timestamp = if add_time_diff_to_start {
            out.times().start.checked_add(time_diff)
        } else {
            u64::from(&out.times().end).checked_sub(time_diff)
        }
        .ok_or(DecodeError::Other(Blame::TheirFault))?;

        let payload_length = CompactU64::relative_decode(producer, &payload_length_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let payload_digest = PD::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(Entry::new(
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
    RelativeEncodableKnownSize<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &(N, Range3d<MCL, MCC, MPL, S>)) -> usize {
        let (namespace, out) = r;

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

        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let time_diff_len =
            CompactU64(time_diff).relative_len_of_encoding(&time_diff_tag.encoding_width());

        let subspace_len = if self.subspace_id() != &out.subspaces().start {
            self.subspace_id().len_of_encoding()
        } else {
            0
        };

        // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
        let encode_path_relative_to_start = match &out.paths().end {
            RangeEnd::Closed(end_path) => {
                let start_lcp = self.path().longest_common_prefix(&out.paths().start);
                let end_lcp = self.path().longest_common_prefix(end_path);

                start_lcp.component_count() >= end_lcp.component_count()
            }
            RangeEnd::Open => true,
        };

        let path_len = match &out.paths().end {
            RangeEnd::Closed(end_path) => {
                if encode_path_relative_to_start {
                    self.path().relative_len_of_encoding(&out.paths().start)
                } else {
                    self.path().relative_len_of_encoding(end_path)
                }
            }
            RangeEnd::Open => self.path().relative_len_of_encoding(&out.paths().start),
        };

        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::three());
        let payload_length_len = CompactU64(self.payload_length())
            .relative_len_of_encoding(&payload_length_tag.encoding_width());

        let payload_digest_len = self.payload_digest().len_of_encoding();

        1 + subspace_len + path_len + time_diff_len + payload_length_len + payload_digest_len
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
    RelativeDecodableSync<(N, Range3d<MCL, MCC, MPL, S>), Blame> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableCanonic,
    S: SubspaceId + DecodableCanonic,
    PD: PayloadDigest + DecodableCanonic,
    Blame: From<N::ErrorReason>
        + From<S::ErrorReason>
        + From<PD::ErrorReason>
        + From<N::ErrorCanonic>
        + From<S::ErrorCanonic>
        + From<PD::ErrorCanonic>,
{
}
