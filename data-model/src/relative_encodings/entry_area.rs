// Entry <> Entry

use compact_u64::{CompactU64, Tag, TagWidth};
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync, RelativeEncodable,
    RelativeEncodableKnownSize, RelativeEncodableSync,
};
use willow_encoding::is_bitflagged;

use crate::{
    grouping::{Area, AreaSubspace},
    Entry, NamespaceId, Path, PayloadDigest, SubspaceId,
};

// Entry <> Area

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodable<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Encodable,
    S: SubspaceId + Encodable,
    PD: PayloadDigest + Encodable,
{
    /// Encodes this [`Entry`] relative to a reference [`NamespaceId`] and [`Area`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_area).
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(N, Area<MCL, MCC, MPL, S>),
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = r;

        if self.namespace_id() != namespace {
            panic!("Tried to encode an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to encode an entry relative to an area it is not included by")
        }

        let time_diff = core::cmp::min(
            self.timestamp() - out.times().start,
            u64::from(&out.times().end) - self.timestamp(),
        );

        let mut header = 0b0000_0000;

        if out.subspace().is_any() {
            header |= 0b1000_0000;
        }

        if self.timestamp() - out.times().start <= u64::from(&out.times().end) - self.timestamp() {
            header |= 0b0100_0000;
        }

        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::two());

        header |= time_diff_tag.data_at_offset(2);
        header |= payload_length_tag.data_at_offset(4);

        consumer.consume(header).await?;

        if out.subspace().is_any() {
            self.subspace_id().encode(consumer).await?;
        }

        self.path().relative_encode(consumer, out.path()).await?;

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
    RelativeDecodable<(N, Area<MCL, MCC, MPL, S>), Blame> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Clone,
    S: SubspaceId + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
    PD: PayloadDigest + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
{
    /// Decodes an [`Entry`] relative to a reference [`NamespaceId`] and [`Area`].
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_area).
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(N, Area<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<false, MCL, MCC, MPL, N, S, PD, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableCanonic<(N, Area<MCL, MCC, MPL, S>), Blame, Blame>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Clone,
    S: SubspaceId + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
    PD: PayloadDigest + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &(N, Area<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<true, MCL, MCC, MPL, N, S, PD, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableKnownSize<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &(N, Area<MCL, MCC, MPL, S>)) -> usize {
        let (namespace, out) = r;

        if self.namespace_id() != namespace {
            panic!("Tried to get the encoded length of an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to get the encoded length of an entry relative to an area it is not included by")
        }

        let time_diff = core::cmp::min(
            self.timestamp() - out.times().start,
            u64::from(&out.times().end) - self.timestamp(),
        );
        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let time_diff_len =
            CompactU64(time_diff).relative_len_of_encoding(&time_diff_tag.encoding_width());

        let subspace_len = if out.subspace().is_any() {
            self.subspace_id().len_of_encoding()
        } else {
            0
        };

        let path_len = self.path().relative_len_of_encoding(out.path());

        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::two());
        let payload_length_len = CompactU64(self.payload_length())
            .relative_len_of_encoding(&payload_length_tag.encoding_width());

        let payload_digest_len = self.payload_digest().len_of_encoding();

        1 + subspace_len + path_len + time_diff_len + payload_length_len + payload_digest_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableSync<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableSync,
    S: SubspaceId + EncodableSync,
    PD: PayloadDigest + EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableSync<(N, Area<MCL, MCC, MPL, S>), Blame> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Clone,
    S: SubspaceId + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
    PD: PayloadDigest + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
{
}

async fn relative_decode_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    P,
>(
    producer: &mut P,
    r: &(N, Area<MCL, MCC, MPL, S>),
) -> Result<Entry<MCL, MCC, MPL, N, S, PD>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
    N: Clone,
    S: SubspaceId + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
    PD: PayloadDigest + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>,
{
    let (namespace, out) = r;

    let header = producer.produce_item().await?;

    let is_subspace_encoded = is_bitflagged(header, 0);
    let add_time_diff_to_start = is_bitflagged(header, 1);
    let time_diff_tag = Tag::from_raw(header, TagWidth::two(), 2);
    let payload_length_tag = Tag::from_raw(header, TagWidth::two(), 4);

    if is_bitflagged(header, 6) || is_bitflagged(header, 7) {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let subspace_id = if is_subspace_encoded {
        match &out.subspace() {
            AreaSubspace::Any => {
                if CANONIC {
                    S::decode_canonic(producer).await?
                } else {
                    S::decode(producer).await?
                }
            }
            AreaSubspace::Id(_) => return Err(DecodeError::Other(Blame::TheirFault)),
        }
    } else {
        match &out.subspace() {
            AreaSubspace::Any => return Err(DecodeError::Other(Blame::TheirFault)),
            AreaSubspace::Id(id) => id.clone(),
        }
    };

    if is_subspace_encoded && r.1.subspace() == &subspace_id {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let path = if CANONIC {
        Path::<MCL, MCC, MPL>::relative_decode_canonic(producer, r.1.path()).await?
    } else {
        Path::<MCL, MCC, MPL>::relative_decode(producer, r.1.path()).await?
    };

    if !path.is_prefixed_by(out.path()) {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let time_diff = if CANONIC {
        CompactU64::relative_decode_canonic(producer, &time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    } else {
        CompactU64::relative_decode(producer, &time_diff_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    };

    let timestamp = if add_time_diff_to_start {
        out.times().start.checked_add(time_diff)
    } else {
        u64::from(&out.times().end).checked_sub(time_diff)
    }
    .ok_or(DecodeError::Other(Blame::TheirFault))?;

    // === Necessary to produce canonic encodings. ===
    // Verify that the correct add_or_subtract_time_diff flag was set.
    if CANONIC {
        let should_have_added = timestamp.checked_sub(out.times().start)
            <= u64::from(&out.times().end).checked_sub(timestamp);

        if add_time_diff_to_start != should_have_added {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
    }

    if !out.times().includes(&timestamp) {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let payload_length = if CANONIC {
        CompactU64::relative_decode_canonic(producer, &payload_length_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    } else {
        CompactU64::relative_decode(producer, &payload_length_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0
    };

    let payload_digest = if CANONIC {
        PD::decode_canonic(producer).await?
    } else {
        PD::decode(producer).await?
    };

    Ok(Entry::new(
        namespace.clone(),
        subspace_id,
        path,
        timestamp,
        payload_length,
        payload_digest,
    ))
}
