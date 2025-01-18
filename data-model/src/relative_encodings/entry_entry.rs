// Entry <> Entry

use compact_u64::*;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync, RelativeEncodable,
    RelativeEncodableKnownSize, RelativeEncodableSync,
};
use willow_encoding::is_bitflagged;

use crate::{Entry, NamespaceId, Path, PayloadDigest, SubspaceId};

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodable<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Encodable,
    S: SubspaceId + Encodable,
    PD: PayloadDigest + Encodable,
{
    /// Encodes this [`Entry`] relative to a reference [`Entry`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_encode<Consumer>(
        &self,
        consumer: &mut Consumer,
        reference: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let time_diff = self.timestamp().abs_diff(reference.timestamp());

        let mut header: u8 = 0b0000_0000;

        if self.namespace_id() != reference.namespace_id() {
            header |= 0b1000_0000;
        }

        if self.subspace_id() != reference.subspace_id() {
            header |= 0b0100_0000;
        }

        if self.timestamp() > reference.timestamp() {
            header |= 0b0010_0000;
        }

        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::two());

        header |= time_diff_tag.data_at_offset(4);
        header |= payload_length_tag.data_at_offset(6);

        consumer.consume(header).await?;

        if self.namespace_id() != reference.namespace_id() {
            self.namespace_id().encode(consumer).await?;
        }

        if self.subspace_id() != reference.subspace_id() {
            self.subspace_id().encode(consumer).await?;
        }

        self.path()
            .relative_encode(consumer, reference.path())
            .await?;

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
    RelativeDecodable<Entry<MCL, MCC, MPL, N, S, PD>, Blame> for Entry<MCL, MCC, MPL, N, S, PD>
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
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<false, MCL, MCC, MPL, N, S, PD, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableCanonic<Entry<MCL, MCC, MPL, N, S, PD>, Blame, Blame>
    for Entry<MCL, MCC, MPL, N, S, PD>
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
    /// Decodes an [`Entry`] relative to the given reference [`Entry`].
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<true, MCL, MCC, MPL, N, S, PD, P>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableKnownSize<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Entry<MCL, MCC, MPL, N, S, PD>) -> usize {
        let namespace_len = if self.namespace_id() != r.namespace_id() {
            self.namespace_id().len_of_encoding()
        } else {
            0
        };

        let subspace_len = if self.subspace_id() != r.subspace_id() {
            self.subspace_id().len_of_encoding()
        } else {
            0
        };

        let path_len = self.path().relative_len_of_encoding(r.path());

        let time_diff = self.timestamp().abs_diff(r.timestamp());
        let time_diff_tag = Tag::min_tag(time_diff, TagWidth::two());
        let time_diff_len =
            CompactU64(time_diff).relative_len_of_encoding(&time_diff_tag.encoding_width());

        let payload_length_tag = Tag::min_tag(self.payload_length(), TagWidth::two());
        let payload_length_len = CompactU64(self.payload_length())
            .relative_len_of_encoding(&payload_length_tag.encoding_width());

        let payload_digest_len = self.payload_digest().len_of_encoding();

        1 + namespace_len
            + subspace_len
            + path_len
            + time_diff_len
            + payload_length_len
            + payload_digest_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableSync<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableSync,
    S: SubspaceId + EncodableSync,
    PD: PayloadDigest + EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableSync<Entry<MCL, MCC, MPL, N, S, PD>, Blame> for Entry<MCL, MCC, MPL, N, S, PD>
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
    r: &Entry<MCL, MCC, MPL, N, S, PD>,
) -> Result<Entry<MCL, MCC, MPL, N, S, PD>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
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
    let header = producer.produce_item().await?;

    // Verify that bit 3 is 0 as specified.
    if CANONIC && is_bitflagged(header, 3) {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let is_namespace_encoded = is_bitflagged(header, 0);
    let is_subspace_encoded = is_bitflagged(header, 1);
    let add_or_subtract_time_diff = is_bitflagged(header, 2);
    let time_diff_tag = Tag::from_raw(header, TagWidth::two(), 4);
    let payload_length_tag = Tag::from_raw(header, TagWidth::two(), 6);

    let namespace_id = if is_namespace_encoded {
        if CANONIC {
            N::decode_canonic(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            N::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        }
    } else {
        r.namespace_id().clone()
    };

    // Verify that the encoded namespace wasn't the same as ours
    // Which would indicate non-canonic encoding
    if CANONIC && is_namespace_encoded && &namespace_id == r.namespace_id() {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let subspace_id = if is_subspace_encoded {
        if CANONIC {
            S::decode_canonic(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            S::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        }
    } else {
        r.subspace_id().clone()
    };

    // Verify that the encoded subspace wasn't the same as ours
    // Which would indicate non-canonic encoding
    if CANONIC && is_subspace_encoded && &subspace_id == r.subspace_id() {
        return Err(DecodeError::Other(Blame::TheirFault));
    }

    let path = if CANONIC {
        Path::<MCL, MCC, MPL>::relative_decode_canonic(producer, r.path()).await?
    } else {
        Path::<MCL, MCC, MPL>::relative_decode(producer, r.path()).await?
    };

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

    // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
    let timestamp = if add_or_subtract_time_diff {
        r.timestamp()
            .checked_add(time_diff)
            .ok_or(DecodeError::Other(Blame::TheirFault))?
    } else {
        r.timestamp()
            .checked_sub(time_diff)
            .ok_or(DecodeError::Other(Blame::TheirFault))?
    };

    // Verify that the correct add_or_subtract_time_diff flag was set.
    if CANONIC {
        let should_have_subtracted = timestamp <= r.timestamp();
        if add_or_subtract_time_diff && should_have_subtracted {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
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
        PD::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?
    } else {
        PD::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?
    };

    Ok(Entry::new(
        namespace_id,
        subspace_id,
        path,
        timestamp,
        payload_length,
        payload_digest,
    ))
}
