// Entry <> Entry

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, DecodeError, Blame, Encodable,
    EncodableKnownSize, EncodableSync, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use crate::{grouping::Area, Entry, NamespaceId, PayloadDigest, SubspaceId};

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
        /*
        let (namespace, out) = reference;

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

        header |= CompactWidth::from_u64(time_diff).bitmask(2);
        header |= CompactWidth::from_u64(self.payload_length()).bitmask(4);

        consumer.consume(header).await?;

        if out.subspace().is_any() {
            self.subspace_id().encode(consumer).await?;
        }

        self.path().relative_encode(out.path(), consumer).await?;
        encode_compact_width_be(time_diff, consumer).await?;
        encode_compact_width_be(self.payload_length(), consumer).await?;
        self.payload_digest().encode(consumer).await?;

        Ok(())
        */

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodable<(N, Area<MCL, MCC, MPL, S>), Blame>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Decodable,
    S: SubspaceId + Decodable + std::fmt::Debug,
    PD: PayloadDigest + Decodable,
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
        /*
        let (namespace, out) = reference;

        let header = produce_byte(producer).await?;

        let is_subspace_encoded = is_bitflagged(header, 0);
        let add_time_diff_to_start = is_bitflagged(header, 1);
        let time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 2);
        let payload_length_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);

        if is_bitflagged(header, 6) || is_bitflagged(header, 7) {
            return Err(DecodeError::InvalidInput);
        }

        let subspace_id = if is_subspace_encoded {
            match &out.subspace() {
                AreaSubspace::Any => S::decode_canonical(producer).await?,
                AreaSubspace::Id(_) => return Err(DecodeError::InvalidInput),
            }
        } else {
            match &out.subspace() {
                AreaSubspace::Any => return Err(DecodeError::InvalidInput),
                AreaSubspace::Id(id) => id.clone(),
            }
        };

        let path = Path::relative_decode_canonical(out.path(), producer).await?;

        if !path.is_prefixed_by(out.path()) {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff = decode_compact_width_be(time_diff_compact_width, producer).await?;
        let payload_length =
            decode_compact_width_be(payload_length_compact_width, producer).await?;

        let payload_digest = PD::decode_canonical(producer).await?;

        let timestamp = if add_time_diff_to_start {
            out.times().start.checked_add(time_diff)
        } else {
            u64::from(&out.times().end).checked_sub(time_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // === Necessary to produce canonic encodings. ===
        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_added = timestamp.checked_sub(out.times().start)
            <= u64::from(&out.times().end).checked_sub(timestamp);

        if add_time_diff_to_start != should_have_added {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

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
    RelativeDecodableCanonic<(N, Area<MCL, MCC, MPL, S>), Blame, Blame>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableCanonic,
    S: SubspaceId + DecodableCanonic + std::fmt::Debug,
    PD: PayloadDigest + DecodableCanonic,
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &(N, Area<MCL, MCC, MPL, S>),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
        let (namespace, out) = reference;

        let header = produce_byte(producer).await?;

        let is_subspace_encoded = is_bitflagged(header, 0);
        let add_time_diff_to_start = is_bitflagged(header, 1);
        let time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 2);
        let payload_length_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);

        let subspace_id = match &out.subspace() {
            AreaSubspace::Any => {
                if !is_subspace_encoded {
                    return Err(DecodeError::InvalidInput);
                }

                S::decode_relation(producer).await?
            }
            AreaSubspace::Id(out_subspace) => {
                if !is_subspace_encoded {
                    out_subspace.clone()
                } else {
                    let subspace_id = S::decode_relation(producer).await?;

                    if &subspace_id != out_subspace {
                        return Err(DecodeError::InvalidInput);
                    }

                    subspace_id
                }
            }
        };

        let path = Path::relative_decode_canonical(out.path(), producer).await?;

        if !path.is_prefixed_by(out.path()) {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff = decode_compact_width_be_relation(time_diff_compact_width, producer).await?;
        let payload_length =
            decode_compact_width_be_relation(payload_length_compact_width, producer).await?;

        let payload_digest = PD::decode_relation(producer).await?;

        let timestamp = if add_time_diff_to_start {
            out.times().start.checked_add(time_diff)
        } else {
            u64::from(&out.times().end).checked_sub(time_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

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
    RelativeEncodableKnownSize<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableKnownSize,
    S: SubspaceId + EncodableKnownSize,
    PD: PayloadDigest + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &(N, Area<MCL, MCC, MPL, S>)) -> usize {
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodableSync<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + EncodableSync,
    S: SubspaceId + EncodableSync + std::fmt::Debug,
    PD: PayloadDigest + EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodableSync<(N, Area<MCL, MCC, MPL, S>), Blame>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + DecodableSync,
    S: SubspaceId + DecodableSync + std::fmt::Debug,
    PD: PayloadDigest + DecodableSync,
{
}
