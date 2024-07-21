use core::future::Future;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        bytes::{is_bitflagged, produce_byte},
        compact_width::{decode_compact_width_be, encode_compact_width_be, CompactWidth},
        error::{DecodeError, EncodingConsumerError},
        max_power::{decode_max_power, encode_max_power},
        parameters::{Decodable, Encodable},
    },
    entry::Entry,
    grouping::{
        area::{Area, AreaSubspace},
        range::RangeEnd,
        range_3d::Range3d,
    },
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

/// A type that can be used to encoded to a bytestring *encoded relative to `R`*.
pub trait RelativeEncodable<R> {
    /// A function from the set `Self` to the set of bytestrings *encoded relative to `reference`*.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), EncodingConsumerError<Consumer::Error>>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be used to decode `T` from a bytestring *encoded relative to `Self`*.
pub trait RelativeDecodable<R> {
    /// A function from the set of bytestrings *encoded relative to `Self`* to the set of `T` in relation to `Self`.
    fn relative_decode<Producer>(
        reference: &R,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

// Path <> Path

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeEncodable<Path<MCL, MCC, MPL>>
    for Path<MCL, MCC, MPL>
{
    /// Encode a path relative to another path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_encode<Consumer>(
        &self,
        reference: &Path<MCL, MCC, MPL>,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let lcp = self.longest_common_prefix(reference);
        encode_max_power(lcp.get_component_count(), MCC, consumer).await?;

        if lcp.get_component_count() > 0 {
            let suffix_components = self.suffix_components(lcp.get_component_count());

            // TODO: A more performant version of this.

            let mut suffix = Path::<MCL, MCC, MPL>::new_empty();

            for component in suffix_components {
                // We can unwrap here because this suffix is a subset of a valid path.
                suffix = suffix.append(component).unwrap();
            }

            suffix.encode(consumer).await?;

            return Ok(());
        }

        self.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeDecodable<Path<MCL, MCC, MPL>>
    for Path<MCL, MCC, MPL>
{
    /// Decode a path relative to this path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_decode<Producer>(
        reference: &Path<MCL, MCC, MPL>,
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let lcp = decode_max_power(MCC, producer).await?;

        if lcp > reference.get_component_count() as u64 {
            return Err(DecodeError::InvalidInput);
        }

        let prefix = reference
            .create_prefix(lcp as usize)
            .ok_or(DecodeError::InvalidInput)?;
        let suffix = Path::<MCL, MCC, MPL>::decode(producer).await?;

        let mut new = prefix;

        for component in suffix.components() {
            match new.append(component) {
                Ok(appended) => new = appended,
                Err(_) => return Err(DecodeError::InvalidInput),
            }
        }

        let actual_lcp = reference.longest_common_prefix(&new);

        if actual_lcp.get_component_count() != lcp as usize {
            return Err(DecodeError::InvalidInput);
        }

        Ok(new)
    }
}

// Entry <> Entry

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodable<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Encodable,
    S: SubspaceId + Encodable,
    PD: PayloadDigest + Encodable,
{
    /// Encode an [`Entry`] relative to a reference [`Entry`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_encode<Consumer>(
        &self,
        reference: &Entry<MCL, MCC, MPL, N, S, PD>,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
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

        consumer.consume(header).await?;

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
    RelativeDecodable<Entry<MCL, MCC, MPL, N, S, PD>> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Decodable + std::fmt::Debug,
    S: SubspaceId + Decodable + std::fmt::Debug,
    PD: PayloadDigest + Decodable,
{
    /// Decode an [`Entry`] relative from this [`Entry`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_etry_relative_entry).
    async fn relative_decode<Producer>(
        reference: &Entry<MCL, MCC, MPL, N, S, PD>,
        producer: &mut Producer,
    ) -> Result<Entry<MCL, MCC, MPL, N, S, PD>, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
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
            N::decode(producer).await?
        } else {
            reference.namespace_id.clone()
        };

        // Verify that the encoded namespace wasn't the same as ours
        // Which would indicate invalid input
        if is_namespace_encoded && namespace_id == reference.namespace_id {
            return Err(DecodeError::InvalidInput);
        }

        let subspace_id = if is_subspace_encoded {
            S::decode(producer).await?
        } else {
            reference.subspace_id.clone()
        };

        // Verify that the encoded subspace wasn't the same as ours
        // Which would indicate invalid input
        if is_subspace_encoded && subspace_id == reference.subspace_id {
            return Err(DecodeError::InvalidInput);
        }

        let path = Path::<MCL, MCC, MPL>::relative_decode(&reference.path, producer).await?;

        let time_diff = decode_compact_width_be(compact_width_time_diff, producer).await?;

        // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
        let timestamp = if add_or_subtract_time_diff {
            reference
                .timestamp
                .checked_add(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        } else {
            reference
                .timestamp
                .checked_sub(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        };

        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_subtracted = timestamp <= reference.timestamp;
        if add_or_subtract_time_diff && should_have_subtracted {
            return Err(DecodeError::InvalidInput);
        }

        let payload_length =
            decode_compact_width_be(compact_width_payload_length, producer).await?;

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

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodable<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Encodable,
    S: SubspaceId + Encodable,
    PD: PayloadDigest + Encodable,
{
    /// Encode an [`Entry`] relative to a reference [`NamespaceId`] and [`Area`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_area).
    async fn relative_encode<Consumer>(
        &self,
        reference: &(N, Area<MCL, MCC, MPL, S>),
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = reference;

        if &self.namespace_id != namespace {
            panic!("Tried to encode an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to encode an entry relative to an area it is not included by")
        }

        let time_diff = core::cmp::min(
            self.timestamp - out.times.start,
            u64::from(&out.times.end) - self.timestamp,
        );

        let mut header = 0b0000_0000;

        if out.subspace == AreaSubspace::Any {
            header |= 0b1000_0000;
        }

        if self.timestamp - out.times.start <= u64::from(&out.times.end) - self.timestamp {
            header |= 0b0100_0000;
        }

        header |= CompactWidth::from_u64(time_diff).bitmask(2);
        header |= CompactWidth::from_u64(self.payload_length).bitmask(4);

        consumer.consume(header).await?;

        if out.subspace == AreaSubspace::Any {
            self.subspace_id.encode(consumer).await?;
        }

        self.path.relative_encode(&out.path, consumer).await?;
        encode_compact_width_be(time_diff, consumer).await?;
        encode_compact_width_be(self.payload_length, consumer).await?;
        self.payload_digest.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeDecodable<(N, Area<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Decodable,
    S: SubspaceId + Decodable + std::fmt::Debug,
    PD: PayloadDigest + Decodable,
{
    /// Decode an [`Entry`] relative to a reference [`NamespaceId`] and [`Area`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_area).
    async fn relative_decode<Producer>(
        reference: &(N, Area<MCL, MCC, MPL, S>),
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
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
            match &out.subspace {
                AreaSubspace::Any => S::decode(producer).await?,
                AreaSubspace::Id(_) => return Err(DecodeError::InvalidInput),
            }
        } else {
            match &out.subspace {
                AreaSubspace::Any => return Err(DecodeError::InvalidInput),
                AreaSubspace::Id(id) => id.clone(),
            }
        };

        let path = Path::relative_decode(&out.path, producer).await?;

        if !path.is_prefixed_by(&out.path) {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff = decode_compact_width_be(time_diff_compact_width, producer).await?;
        let payload_length =
            decode_compact_width_be(payload_length_compact_width, producer).await?;

        let payload_digest = PD::decode(producer).await?;

        let timestamp = if add_time_diff_to_start {
            out.times.start.checked_add(time_diff)
        } else {
            u64::from(&out.times.end).checked_sub(time_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_added = timestamp.checked_sub(out.times.start)
            <= u64::from(&out.times.end).checked_sub(timestamp);

        if add_time_diff_to_start != should_have_added {
            return Err(DecodeError::InvalidInput);
        }

        if !out.times.includes(&timestamp) {
            return Err(DecodeError::InvalidInput);
        }

        Ok(Self {
            namespace_id: namespace.clone(),
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        })
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    RelativeEncodable<(N, Range3d<MCL, MCC, MPL, S>)> for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Encodable,
    S: SubspaceId + Encodable,
    PD: PayloadDigest + Encodable,
{
    /// Encode an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
    async fn relative_encode<Consumer>(
        &self,
        reference: &(N, Range3d<MCL, MCC, MPL, S>),
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = reference;

        if &self.namespace_id != namespace {
            panic!("Tried to encode an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to encode an entry relative to a 3d range it is not included by")
        }

        let time_diff = core::cmp::min(
            self.timestamp.abs_diff(out.times.start),
            self.timestamp.abs_diff(u64::from(&out.times.end)),
        );

        let mut header = 0b0000_0000;

        // Encode e.subspace_id?
        if self.subspace_id != out.subspaces.start {
            header |= 0b1000_0000;
        }

        // Encode e.path relative to out.paths.start or to out.paths.end?
        let encode_path_relative_to_start = match &out.paths.end {
            RangeEnd::Closed(end_path) => {
                let start_lcp = self.path.longest_common_prefix(&out.paths.start);
                let end_lcp = self.path.longest_common_prefix(end_path);

                start_lcp.get_component_count() >= end_lcp.get_component_count()
            }
            RangeEnd::Open => true,
        };

        if encode_path_relative_to_start {
            header |= 0b0100_0000;
        }

        // Add time_diff to out.times.start, or subtract from out.times.end?
        let add_time_diff_with_start = time_diff == self.timestamp.abs_diff(out.times.start);

        if add_time_diff_with_start {
            header |= 0b0010_0000;
        }

        // 2-bit integer n such that 2^n gives compact_width(time_diff)
        header |= CompactWidth::from_u64(time_diff).bitmask(4);

        // 2-bit integer n such that 2^n gives compact_width(e.payload_length)
        header |= CompactWidth::from_u64(self.payload_length).bitmask(6);

        consumer.consume(header).await?;

        if self.subspace_id != out.subspaces.start {
            self.subspace_id.encode(consumer).await?;
        }

        // Encode e.path relative to out.paths.start or to out.paths.end?
        match &out.paths.end {
            RangeEnd::Closed(end_path) => {
                if encode_path_relative_to_start {
                    self.path
                        .relative_encode(&out.paths.start, consumer)
                        .await?;
                } else {
                    self.path.relative_encode(end_path, consumer).await?;
                }
            }
            RangeEnd::Open => {
                self.path
                    .relative_encode(&out.paths.start, consumer)
                    .await?;
            }
        }

        encode_compact_width_be(time_diff, consumer).await?;
        encode_compact_width_be(self.payload_length, consumer).await?;
        self.payload_digest.encode(consumer).await?;

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
    /// Encode an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_entry_in_namespace_3drange).
    async fn relative_decode<Producer>(
        reference: &(N, Range3d<MCL, MCC, MPL, S>),
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let (namespace, out) = reference;

        let header = produce_byte(producer).await?;

        // Decode e.subspace_id?
        let is_subspace_encoded = is_bitflagged(header, 0);

        // Decode e.path relative to out.paths.start or to out.paths.end?
        let decode_path_relative_to_start = is_bitflagged(header, 1);

        // Add time_diff to out.times.start, or subtract from out.times.end?
        let add_time_diff_with_start = is_bitflagged(header, 2);

        if is_bitflagged(header, 3) {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 4);
        let payload_length_compact_width = CompactWidth::decode_fixed_width_bitmask(header, 6);

        let subspace_id = if is_subspace_encoded {
            S::decode(producer).await?
        } else {
            out.subspaces.start.clone()
        };

        // Verify that encoding the subspace was necessary.
        if subspace_id == out.subspaces.start && is_subspace_encoded {
            return Err(DecodeError::InvalidInput);
        }

        // Verify that subspace is included by range
        if !out.subspaces.includes(&subspace_id) {
            return Err(DecodeError::InvalidInput);
        }

        let path = if decode_path_relative_to_start {
            Path::relative_decode(&out.paths.start, producer).await?
        } else {
            match &out.paths.end {
                RangeEnd::Closed(end_path) => Path::relative_decode(end_path, producer).await?,
                RangeEnd::Open => return Err(DecodeError::InvalidInput),
            }
        };

        // Verify that path is included by range
        if !out.paths.includes(&path) {
            return Err(DecodeError::InvalidInput);
        }

        // Verify that the path was encoded relative to the correct bound of the referenc path range.
        let should_have_encoded_path_relative_to_start = match &out.paths.end {
            RangeEnd::Closed(end_path) => {
                let start_lcp = path.longest_common_prefix(&out.paths.start);
                let end_lcp = path.longest_common_prefix(end_path);

                start_lcp.get_component_count() >= end_lcp.get_component_count()
            }
            RangeEnd::Open => true,
        };

        if decode_path_relative_to_start != should_have_encoded_path_relative_to_start {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff = decode_compact_width_be(time_diff_compact_width, producer).await?;

        let payload_length =
            decode_compact_width_be(payload_length_compact_width, producer).await?;

        let payload_digest = PD::decode(producer).await?;

        let timestamp = if add_time_diff_with_start {
            out.times.start.checked_add(time_diff)
        } else {
            match &out.times.end {
                RangeEnd::Closed(end_time) => end_time.checked_sub(time_diff),
                RangeEnd::Open => u64::from(&out.times.end).checked_sub(time_diff),
            }
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify that timestamp is included by range
        if !out.times.includes(&timestamp) {
            return Err(DecodeError::InvalidInput);
        }

        // Verify that time_diff is what it should have been
        let correct_time_diff = core::cmp::min(
            timestamp.abs_diff(out.times.start),
            timestamp.abs_diff(u64::from(&out.times.end)),
        );

        if time_diff != correct_time_diff {
            return Err(DecodeError::InvalidInput);
        }

        // Verify that the combine with start bitflag in the header was correct
        let should_have_added_to_start = time_diff == timestamp.abs_diff(out.times.start);

        if should_have_added_to_start != add_time_diff_with_start {
            return Err(DecodeError::InvalidInput);
        }

        Ok(Self {
            namespace_id: namespace.clone(),
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        })
    }
}
