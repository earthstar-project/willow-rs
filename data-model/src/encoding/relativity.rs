use core::future::Future;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        compact_width::{decode_compact_width_be, encode_compact_width_be, CompactWidth},
        error::{DecodeError, EncodingConsumerError},
        max_power::{decode_max_power, encode_max_power},
        parameters::{Decoder, Encoder},
    },
    entry::Entry,
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::{Path, PathRc},
};

/// Returned when a relative encoding fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeEncodeError<E> {
    /// The subject could not be encoded relative to the reference (e.g. because it was not logically included by the reference).
    IllegalRelativity(),
    /// The encoding failed to be consumed by a [`ufotofu::local_nb::Consumer`].
    Consumer(EncodingConsumerError<E>),
}

impl<E> From<EncodingConsumerError<E>> for RelativeEncodeError<E> {
    fn from(err: EncodingConsumerError<E>) -> Self {
        RelativeEncodeError::Consumer(err)
    }
}

/// A type that can be used to encoded to a bytestring *encoded relative to `R`*.
pub trait RelativeEncoder<R> {
    /// A function from the set `Self` to the set of bytestrings *encoded relative to `reference`*.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), RelativeEncodeError<Consumer::Error>>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be used to decode `T` from a bytestring *encoded relative to `Self`*.
pub trait RelativeDecoder<T> {
    /// A function from the set of bytestrings *encoded relative to `Self`* to the set of `T` in relation to `Self`.
    fn relative_decode<Producer>(
        &self,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<T, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

// Path <> Path

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeEncoder<PathRc<MCL, MCC, MPL>>
    for PathRc<MCL, MCC, MPL>
{
    /// Encode a path relative to another path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_encode<Consumer>(
        &self,
        reference: &PathRc<MCL, MCC, MPL>,
        consumer: &mut Consumer,
    ) -> Result<(), RelativeEncodeError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let lcp = self.longest_common_prefix(reference);
        encode_max_power(lcp.component_count(), MCC, consumer).await?;

        let suffix = self.create_suffix(self.component_count() - lcp.component_count());
        suffix.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeDecoder<PathRc<MCL, MCC, MPL>>
    for PathRc<MCL, MCC, MPL>
{
    /// Decode a path relative to this path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_decode<Producer>(
        &self,
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let lcp = decode_max_power(MCC, producer).await?;

        if lcp > self.component_count() as u64 {
            return Err(DecodeError::InvalidInput);
        }

        let prefix = self.create_prefix(lcp as usize);
        let suffix = PathRc::<MCL, MCC, MPL>::decode(producer).await?;

        let mut new = prefix;

        for component in suffix.components() {
            match new.append(component.clone()) {
                Ok(appended) => new = appended,
                Err(_) => return Err(DecodeError::InvalidInput),
            }
        }

        let actual_lcp = self.longest_common_prefix(&new);

        if actual_lcp.component_count() != lcp as usize {
            return Err(DecodeError::InvalidInput);
        }

        Ok(new)
    }
}

// Entry <> Entry

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncoder<Entry<N, S, PathRc<MCL, MCC, MPL>, PD>>
    for Entry<N, S, PathRc<MCL, MCC, MPL>, PD>
where
    N: NamespaceId + Encoder,
    S: SubspaceId + Encoder,
    PD: PayloadDigest + Encoder,
{
    /// Encode an [`Entry`] relative to a reference [`Entry`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_encode<Consumer>(
        &self,
        reference: &Entry<N, S, PathRc<MCL, MCC, MPL>, PD>,
        consumer: &mut Consumer,
    ) -> Result<(), RelativeEncodeError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let time_diff = self.timestamp.abs_diff(reference.timestamp);

        let mut header: u8 = 0b0000_0000;

        if self.namespace_id != reference.namespace_id {
            header |= 0b1000_0000;
        }

        if self.subspace_id != reference.subspace_id {
            header |= 0b0100_0000;
        }

        if self.timestamp > reference.timestamp {
            header |= 0b0010_0000;
        }

        header |= CompactWidth::from_u64(time_diff).bitmask(4);

        header |= CompactWidth::from_u64(self.payload_length).bitmask(6);

        if let Err(err) = consumer.consume(header).await {
            return Err(RelativeEncodeError::Consumer(EncodingConsumerError {
                bytes_consumed: 0,
                reason: err,
            }));
        };

        if self.namespace_id != reference.namespace_id {
            self.namespace_id.encode(consumer).await?;
        }

        if self.subspace_id != reference.subspace_id {
            self.subspace_id.encode(consumer).await?;
        }

        self.path.relative_encode(&reference.path, consumer).await?;

        encode_compact_width_be(time_diff, consumer).await?;

        encode_compact_width_be(self.payload_length, consumer).await?;

        self.payload_digest.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecoder<Entry<N, S, PathRc<MCL, MCC, MPL>, PD>>
    for Entry<N, S, PathRc<MCL, MCC, MPL>, PD>
where
    N: NamespaceId + Decoder + std::fmt::Debug,
    S: SubspaceId + Decoder + std::fmt::Debug,
    PD: PayloadDigest + Decoder,
{
    /// Decode an [`Entry`] relative from this [`Entry`].
    async fn relative_decode<Producer>(
        &self,
        producer: &mut Producer,
    ) -> Result<Entry<N, S, PathRc<MCL, MCC, MPL>, PD>, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut header_slice = [0u8];

        producer
            .bulk_overwrite_full_slice(&mut header_slice)
            .await?;

        let header = header_slice[0];

        // Verify that bit 3 is 0 as specified - good indicator of invalid data.
        if header | 0b1110_1111 == 0b1111_1111 {
            return Err(DecodeError::InvalidInput);
        }

        let is_namespace_encoded = header & 0b1000_0000 == 0b1000_0000;
        let is_subspace_encoded = header & 0b0100_0000 == 0b0100_0000;
        let add_or_subtract_time_diff = header & 0b0010_0000 == 0b0010_0000;
        let compact_width_time_diff = CompactWidth::from_2bit_int(header, 4);
        let compact_width_payload_length = CompactWidth::from_2bit_int(header, 6);

        let namespace_id = if is_namespace_encoded {
            N::decode(producer).await?
        } else {
            self.namespace_id.clone()
        };

        // Verify that the encoded namespace wasn't the same as ours
        // Which would indicate invalid input
        if is_namespace_encoded && namespace_id == self.namespace_id {
            return Err(DecodeError::InvalidInput);
        }

        let subspace_id = if is_subspace_encoded {
            S::decode(producer).await?
        } else {
            self.subspace_id.clone()
        };

        // Verify that the encoded subspace wasn't the same as ours
        // Which would indicate invalid input
        if is_subspace_encoded && subspace_id == self.subspace_id {
            return Err(DecodeError::InvalidInput);
        }

        let path = self.path.relative_decode(producer).await?;

        let time_diff = decode_compact_width_be(compact_width_time_diff, producer).await?;

        // Verify that the compact width of the time diff matches the time diff we just decoded.
        if CompactWidth::from_2bit_int(header, 4) != CompactWidth::from_u64(time_diff) {
            return Err(DecodeError::InvalidInput);
        }

        // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
        let timestamp = if add_or_subtract_time_diff {
            self.timestamp
                .checked_add(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        } else {
            self.timestamp
                .checked_sub(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        };

        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_subtracted = timestamp <= self.timestamp;
        if add_or_subtract_time_diff && should_have_subtracted {
            return Err(DecodeError::InvalidInput);
        }

        let payload_length =
            decode_compact_width_be(compact_width_payload_length, producer).await?;

        // Verify that the compact width of the payload length matches the payload length we just decoded.
        if CompactWidth::from_2bit_int(header, 6) != CompactWidth::from_u64(payload_length) {
            return Err(DecodeError::InvalidInput);
        }

        let payload_digest = PD::decode(producer).await?;

        Ok(Entry {
            namespace_id,
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        })
    }
}
