// Entry <> Entry

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, DecodeError, DecodingWentWrong, Encodable,
    EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use crate::{Entry, NamespaceId, PayloadDigest, SubspaceId};

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
        /*  let time_diff = self.timestamp().abs_diff(reference.timestamp());

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

        header |= CompactWidth::from_u64(time_diff).bitmask(4);

        header |= CompactWidth::from_u64(self.payload_length()).bitmask(6);

        consumer.consume(header).await?;

        if self.namespace_id() != reference.namespace_id() {
            self.namespace_id().encode(consumer).await?;
        }

        if self.subspace_id() != reference.subspace_id() {
            self.subspace_id().encode(consumer).await?;
        }

        self.path()
            .relative_encode(reference.path(), consumer)
            .await?;

        encode_compact_width_be(time_diff, consumer).await?;

        encode_compact_width_be(self.payload_length(), consumer).await?;

        self.payload_digest().encode(consumer).await?;

        Ok(()) */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodable<Entry<MCL, MCC, MPL, N, S, PD>, DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Decodable + std::fmt::Debug,
    S: SubspaceId + Decodable + std::fmt::Debug,
    PD: PayloadDigest + Decodable,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
        let header = produce_byte(producer).await?;

        let is_namespace_encoded = is_bitflagged(header, 0);
        let is_subspace_encoded = is_bitflagged(header, 1);
        let add_or_subtract_time_diff = is_bitflagged(header, 2);
        let compact_width_time_diff = CompactWidth::decode_fixed_width_bitmask(header, 4);
        let compact_width_payload_length = CompactWidth::decode_fixed_width_bitmask(header, 6);

        let namespace_id = if is_namespace_encoded {
            N::decode_relation(producer).await?
        } else {
            reference.namespace_id().clone()
        };

        let subspace_id = if is_subspace_encoded {
            S::decode_relation(producer).await?
        } else {
            reference.subspace_id().clone()
        };

        let path = Path::<MCL, MCC, MPL>::relative_decode_canonical(reference.path(), producer)
            .await?;

        let time_diff =
            decode_compact_width_be_relation(compact_width_time_diff, producer).await?;

        // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
        let timestamp = if add_or_subtract_time_diff {
            reference
                .timestamp()
                .checked_add(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        } else {
            reference
                .timestamp()
                .checked_sub(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        };

        let payload_length =
            decode_compact_width_be_relation(compact_width_payload_length, producer).await?;

        let payload_digest = PD::decode_relation(producer).await?;

        Ok(Entry::new(
            namespace_id,
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
    RelativeDecodableCanonic<Entry<MCL, MCC, MPL, N, S, PD>, DecodingWentWrong, DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableCanonic + std::fmt::Debug,
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    PD: PayloadDigest + DecodableCanonic,
{
    /// Decodes an [`Entry`] relative to the given reference [`Entry`].
    ///
    /// Will return an error if the encoding has not been produced by the corresponding encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
        let header = produce_byte(producer).await?;

        // Verify that bit 3 is 0 as specified.
        if is_bitflagged(header, 3) {
            return Err(DecodeError::InvalidInput);
        }

        let is_namespace_encoded = is_bitflagged(header, 0);
        let is_subspace_encoded = is_bitflagged(header, 1);
        let add_or_subtract_time_diff = is_bitflagged(header, 2);
        let compact_width_time_diff = CompactWidth::decode_fixed_width_bitmask(header, 4);
        let compact_width_payload_length = CompactWidth::decode_fixed_width_bitmask(header, 6);

        let namespace_id = if is_namespace_encoded {
            N::decode_canonical(producer).await?
        } else {
            reference.namespace_id().clone()
        };

        // Verify that the encoded namespace wasn't the same as ours
        // Which would indicate invalid input
        if is_namespace_encoded && &namespace_id == reference.namespace_id() {
            return Err(DecodeError::InvalidInput);
        }

        let subspace_id = if is_subspace_encoded {
            S::decode_canonical(producer).await?
        } else {
            reference.subspace_id().clone()
        };

        // Verify that the encoded subspace wasn't the same as ours
        // Which would indicate invalid input
        if is_subspace_encoded && &subspace_id == reference.subspace_id() {
            return Err(DecodeError::InvalidInput);
        }

        let path = Path::<MCL, MCC, MPL>::relative_decode_canonical(reference.path(), producer)
            .await?;

        let time_diff = decode_compact_width_be(compact_width_time_diff, producer).await?;

        // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
        let timestamp = if add_or_subtract_time_diff {
            reference
                .timestamp()
                .checked_add(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        } else {
            reference
                .timestamp()
                .checked_sub(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        };

        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_subtracted = timestamp <= reference.timestamp();
        if add_or_subtract_time_diff && should_have_subtracted {
            return Err(DecodeError::InvalidInput);
        }

        let payload_length =
            decode_compact_width_be(compact_width_payload_length, producer).await?;

        let payload_digest = PD::decode_canonical(producer).await?;

        Ok(Entry::new(
            namespace_id,
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
    RelativeEncodableKnownSize<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Entry<MCL, MCC, MPL, N, S, PD>) -> usize {
        todo!()
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
    RelativeDecodableSync<Entry<MCL, MCC, MPL, N, S, PD>, DecodingWentWrong>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableSync + std::fmt::Debug,
    S: SubspaceId + DecodableSync + std::fmt::Debug,
    PD: PayloadDigest + DecodableSync + std::fmt::Debug,
{
}
