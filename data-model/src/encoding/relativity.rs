use core::future::Future;
use core::mem::{size_of, MaybeUninit};
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        bytes::{is_bitflagged, produce_byte},
        compact_width::{decode_compact_width_be, encode_compact_width_be, CompactWidth},
        error::DecodeError,
        max_power::{decode_max_power, encode_max_power},
        parameters::{Decodable, Encodable},
    },
    entry::Entry,
    grouping::{
        area::{Area, AreaSubspace},
        range::{Range, RangeEnd},
        range_3d::Range3d,
    },
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

use super::shared_buffers::ScratchSpacePathDecoding;

/// A type that can be used to encode `T` to a bytestring *encoded relative to `R`*.
/// This can be used to create more compact encodings from which `T` can be derived by anyone with `R`.
pub trait RelativeEncodable<R> {
    /// A function from the set `Self` to the set of bytestrings *encoded relative to `reference`*.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be used to decode `T` from a bytestring *encoded relative to `Self`*.
/// This can be used to decode a compact encoding frow which `T` can be derived by anyone with `R`.
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
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let lcp = self.longest_common_prefix(reference);
        let lcp_component_count = lcp.get_component_count();
        encode_max_power(lcp_component_count, MCC, consumer).await?;

        let suffix_component_count = self.get_component_count() - lcp_component_count;
        encode_max_power(suffix_component_count, MCC, consumer).await?;

        for component in self.suffix_components(lcp_component_count) {
            encode_max_power(component.len(), MCL, consumer).await?;

            consumer
                .bulk_consume_full_slice(component.as_ref())
                .await
                .map_err(|f| f.reason)?;
        }

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
        let lcp_component_count: usize = decode_max_power(MCC, producer).await?.try_into()?;

        if lcp_component_count == 0 {
            let decoded = Path::<MCL, MCC, MPL>::decode(producer).await?;

            // === Necessary to produce canonic encodings. ===
            if lcp_component_count
                != decoded
                    .longest_common_prefix(reference)
                    .get_component_count()
            {
                return Err(DecodeError::InvalidInput);
            }
            // ===============================================

            return Ok(decoded);
        }

        let prefix = reference
            .create_prefix(lcp_component_count as usize)
            .ok_or(DecodeError::InvalidInput)?;

        let mut buf = ScratchSpacePathDecoding::<MCC, MPL>::new();

        // Copy the accumulated component lengths of the prefix into the scratch buffer.
        let raw_prefix_acc_component_lengths =
            &prefix.raw_buf()[size_of::<usize>()..size_of::<usize>() * (lcp_component_count + 1)];
        unsafe {
            // Safe because len is less than size_of::<usize>() times the MCC, because `prefix` respects the MCC.
            buf.set_many_component_accumulated_lengths_from_ne(raw_prefix_acc_component_lengths);
        }

        // Copy the raw path data of the prefix into the scratch buffer.
        unsafe {
            // safe because we just copied the accumulated component lengths for the first `lcp_component_count` components.
            MaybeUninit::copy_from_slice(
                buf.path_data_until_as_mut(lcp_component_count),
                &reference.raw_buf()[size_of::<usize>() * (reference.get_component_count() + 1)
                    ..size_of::<usize>() * (reference.get_component_count() + 1)
                        + prefix.get_path_length()],
            );
        }

        let remaining_component_count: usize = decode_max_power(MCC, producer).await?.try_into()?;
        let total_component_count = lcp_component_count + remaining_component_count;
        if total_component_count > MCC {
            return Err(DecodeError::InvalidInput);
        }

        let mut accumulated_component_length: usize = prefix.get_path_length(); // Always holds the acc length of all components we copied so far.
        for i in lcp_component_count..total_component_count {
            let component_len: usize = decode_max_power(MCL, producer).await?.try_into()?;
            if component_len > MCL {
                return Err(DecodeError::InvalidInput);
            }

            accumulated_component_length += component_len;
            if accumulated_component_length > MPL {
                return Err(DecodeError::InvalidInput);
            }

            buf.set_component_accumulated_length(accumulated_component_length, i);

            // Decode the component itself into the scratch buffer.
            producer
                .bulk_overwrite_full_slice_uninit(unsafe {
                    // Safe because we called set_component_Accumulated_length for all j <= i
                    buf.path_data_as_mut(i)
                })
                .await?;
        }

        let decoded = unsafe { buf.to_path(total_component_count) };

        // === Necessary to produce canonic encodings. ===
        if lcp_component_count
            != decoded
                .longest_common_prefix(reference)
                .get_component_count()
        {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        Ok(decoded)
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
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let time_diff = self.get_timestamp().abs_diff(reference.get_timestamp());

        let mut header: u8 = 0b0000_0000;

        if self.get_namespace_id() != reference.get_namespace_id() {
            header |= 0b1000_0000;
        }

        if self.get_subspace_id() != reference.get_subspace_id() {
            header |= 0b0100_0000;
        }

        if self.get_timestamp() > reference.get_timestamp() {
            header |= 0b0010_0000;
        }

        header |= CompactWidth::from_u64(time_diff).bitmask(4);

        header |= CompactWidth::from_u64(self.get_payload_length()).bitmask(6);

        consumer.consume(header).await?;

        if self.get_namespace_id() != reference.get_namespace_id() {
            self.get_namespace_id().encode(consumer).await?;
        }

        if self.get_subspace_id() != reference.get_subspace_id() {
            self.get_subspace_id().encode(consumer).await?;
        }

        self.get_path()
            .relative_encode(&reference.get_path(), consumer)
            .await?;

        encode_compact_width_be(time_diff, consumer).await?;

        encode_compact_width_be(self.get_payload_length(), consumer).await?;

        self.get_payload_digest().encode(consumer).await?;

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
            reference.get_namespace_id().clone()
        };

        /*
        // Verify that the encoded namespace wasn't the same as ours
        // Which would indicate invalid input
        if is_namespace_encoded && namespace_id == reference.get_namespace_id() {
            return Err(DecodeError::InvalidInput);
        }
        */

        let subspace_id = if is_subspace_encoded {
            S::decode(producer).await?
        } else {
            reference.get_subspace_id().clone()
        };

        /*
        // Verify that the encoded subspace wasn't the same as ours
        // Which would indicate invalid input
        if is_subspace_encoded && subspace_id == reference.get_subspace_id() {
            return Err(DecodeError::InvalidInput);
        }
        */

        let path = Path::<MCL, MCC, MPL>::relative_decode(&reference.get_path(), producer).await?;

        let time_diff = decode_compact_width_be(compact_width_time_diff, producer).await?;

        // Add or subtract safely here to avoid overflows caused by malicious or faulty encodings.
        let timestamp = if add_or_subtract_time_diff {
            reference
                .get_timestamp()
                .checked_add(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        } else {
            reference
                .get_timestamp()
                .checked_sub(time_diff)
                .ok_or(DecodeError::InvalidInput)?
        };

        /*
        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_subtracted = timestamp <= reference.get_timestamp();
        if add_or_subtract_time_diff && should_have_subtracted {
            return Err(DecodeError::InvalidInput);
        }
        */

        let payload_length =
            decode_compact_width_be(compact_width_payload_length, producer).await?;

        let payload_digest = PD::decode(producer).await?;

        Ok(Entry::new(
            namespace_id,
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        ))
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
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = reference;

        if self.get_namespace_id() != namespace {
            panic!("Tried to encode an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to encode an entry relative to an area it is not included by")
        }

        let time_diff = core::cmp::min(
            self.get_timestamp() - out.get_times().start,
            u64::from(&out.get_times().end) - self.get_timestamp(),
        );

        let mut header = 0b0000_0000;

        if out.get_subspace().is_any() {
            header |= 0b1000_0000;
        }

        if self.get_timestamp() - out.get_times().start
            <= u64::from(&out.get_times().end) - self.get_timestamp()
        {
            header |= 0b0100_0000;
        }

        header |= CompactWidth::from_u64(time_diff).bitmask(2);
        header |= CompactWidth::from_u64(self.get_payload_length()).bitmask(4);

        consumer.consume(header).await?;

        if out.get_subspace().is_any() {
            self.get_subspace_id().encode(consumer).await?;
        }

        self.get_path()
            .relative_encode(&out.get_path(), consumer)
            .await?;
        encode_compact_width_be(time_diff, consumer).await?;
        encode_compact_width_be(self.get_payload_length(), consumer).await?;
        self.get_payload_digest().encode(consumer).await?;

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
            match &out.get_subspace() {
                AreaSubspace::Any => S::decode(producer).await?,
                AreaSubspace::Id(_) => return Err(DecodeError::InvalidInput),
            }
        } else {
            match &out.get_subspace() {
                AreaSubspace::Any => return Err(DecodeError::InvalidInput),
                AreaSubspace::Id(id) => id.clone(),
            }
        };

        let path = Path::relative_decode(&out.get_path(), producer).await?;

        if !path.is_prefixed_by(&out.get_path()) {
            return Err(DecodeError::InvalidInput);
        }

        let time_diff = decode_compact_width_be(time_diff_compact_width, producer).await?;
        let payload_length =
            decode_compact_width_be(payload_length_compact_width, producer).await?;

        let payload_digest = PD::decode(producer).await?;

        let timestamp = if add_time_diff_to_start {
            out.get_times().start.checked_add(time_diff)
        } else {
            u64::from(&out.get_times().end).checked_sub(time_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // === Necessary to produce canonic encodings. ===
        // Verify that the correct add_or_subtract_time_diff flag was set.
        let should_have_added = timestamp.checked_sub(out.get_times().start)
            <= u64::from(&out.get_times().end).checked_sub(timestamp);

        if add_time_diff_to_start != should_have_added {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        if !out.get_times().includes(&timestamp) {
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
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let (namespace, out) = reference;

        if self.get_namespace_id() != namespace {
            panic!("Tried to encode an entry relative to a namespace it does not belong to")
        }

        if !out.includes_entry(self) {
            panic!("Tried to encode an entry relative to a 3d range it is not included by")
        }

        let time_diff = core::cmp::min(
            self.get_timestamp().abs_diff(out.get_times().start),
            self.get_timestamp()
                .abs_diff(u64::from(&out.get_times().end)),
        );

        let mut header = 0b0000_0000;

        // Encode e.get_subspace_id()?
        if self.get_subspace_id() != &out.get_subspaces().start {
            header |= 0b1000_0000;
        }

        // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
        let encode_path_relative_to_start = match &out.get_paths().end {
            RangeEnd::Closed(end_path) => {
                let start_lcp = self
                    .get_path()
                    .longest_common_prefix(&out.get_paths().start);
                let end_lcp = self.get_path().longest_common_prefix(end_path);

                start_lcp.get_component_count() >= end_lcp.get_component_count()
            }
            RangeEnd::Open => true,
        };

        if encode_path_relative_to_start {
            header |= 0b0100_0000;
        }

        // Add time_diff to out.get_times().start, or subtract from out.get_times().end?
        let add_time_diff_with_start =
            time_diff == self.get_timestamp().abs_diff(out.get_times().start);

        if add_time_diff_with_start {
            header |= 0b0010_0000;
        }

        // 2-bit integer n such that 2^n gives compact_width(time_diff)
        header |= CompactWidth::from_u64(time_diff).bitmask(4);

        // 2-bit integer n such that 2^n gives compact_width(e.get_payload_length())
        header |= CompactWidth::from_u64(self.get_payload_length()).bitmask(6);

        consumer.consume(header).await?;

        if self.get_subspace_id() != &out.get_subspaces().start {
            self.get_subspace_id().encode(consumer).await?;
        }

        // Encode e.get_path() relative to out.get_paths().start or to out.get_paths().end?
        match &out.get_paths().end {
            RangeEnd::Closed(end_path) => {
                if encode_path_relative_to_start {
                    self.get_path()
                        .relative_encode(&out.get_paths().start, consumer)
                        .await?;
                } else {
                    self.get_path().relative_encode(end_path, consumer).await?;
                }
            }
            RangeEnd::Open => {
                self.get_path()
                    .relative_encode(&out.get_paths().start, consumer)
                    .await?;
            }
        }

        encode_compact_width_be(time_diff, consumer).await?;
        encode_compact_width_be(self.get_payload_length(), consumer).await?;
        self.get_payload_digest().encode(consumer).await?;

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
    /// Decode an [`Entry`] relative to a reference [`NamespaceId`] and [`Range3d`].
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
            S::decode(producer).await?
        } else {
            out.get_subspaces().start.clone()
        };

        // === Necessary to produce canonic encodings. ===
        // Verify that encoding the subspace was necessary.
        if subspace_id == out.get_subspaces().start && is_subspace_encoded {
            return Err(DecodeError::InvalidInput);
        }
        // ===============================================

        // Verify that subspace is included by range
        if !out.get_subspaces().includes(&subspace_id) {
            return Err(DecodeError::InvalidInput);
        }

        let path = if decode_path_relative_to_start {
            Path::relative_decode(&out.get_paths().start, producer).await?
        } else {
            match &out.get_paths().end {
                RangeEnd::Closed(end_path) => Path::relative_decode(end_path, producer).await?,
                RangeEnd::Open => return Err(DecodeError::InvalidInput),
            }
        };

        // Verify that path is included by range
        if !out.get_paths().includes(&path) {
            return Err(DecodeError::InvalidInput);
        }

        // === Necessary to produce canonic encodings. ===
        // Verify that the path was encoded relative to the correct bound of the referenc path range.
        let should_have_encoded_path_relative_to_start = match &out.get_paths().end {
            RangeEnd::Closed(end_path) => {
                let start_lcp = path.longest_common_prefix(&out.get_paths().start);
                let end_lcp = path.longest_common_prefix(end_path);

                start_lcp.get_component_count() >= end_lcp.get_component_count()
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

        let payload_digest = PD::decode(producer).await?;

        let timestamp = if add_time_diff_with_start {
            out.get_times().start.checked_add(time_diff)
        } else {
            match &out.get_times().end {
                RangeEnd::Closed(end_time) => end_time.checked_sub(time_diff),
                RangeEnd::Open => u64::from(&out.get_times().end).checked_sub(time_diff),
            }
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify that timestamp is included by range
        if !out.get_times().includes(&timestamp) {
            return Err(DecodeError::InvalidInput);
        }

        // === Necessary to produce canonic encodings. ===
        // Verify that time_diff is what it should have been
        let correct_time_diff = core::cmp::min(
            timestamp.abs_diff(out.get_times().start),
            timestamp.abs_diff(u64::from(&out.get_times().end)),
        );

        if time_diff != correct_time_diff {
            return Err(DecodeError::InvalidInput);
        }

        // Verify that the combine with start bitflag in the header was correct
        let should_have_added_to_start = time_diff == timestamp.abs_diff(out.get_times().start);

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

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodable<Area<MCL, MCC, MPL, S>> for Area<MCL, MCC, MPL, S>
where
    S: SubspaceId + Encodable,
{
    /// Encode an [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
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
            self.get_times().start - out.get_times().start,
            u64::from(&out.get_times().end) - self.get_times().start,
        );

        let end_diff = core::cmp::min(
            u64::from(&self.get_times().end) - out.get_times().start,
            u64::from(&out.get_times().end) - u64::from(&self.get_times().end),
        );

        let mut header = 0;

        if self.get_subspace() != out.get_subspace() {
            header |= 0b1000_0000;
        }

        if self.get_times().end == RangeEnd::Open {
            header |= 0b0100_0000;
        }

        if start_diff == self.get_times().start - out.get_times().start {
            header |= 0b0010_0000;
        }

        if self.get_times().end != RangeEnd::Open
            && end_diff == u64::from(&self.get_times().end) - out.get_times().start
        {
            header |= 0b0001_0000;
        }

        header |= CompactWidth::from_u64(start_diff).bitmask(4);
        header |= CompactWidth::from_u64(end_diff).bitmask(6);

        consumer.consume(header).await?;

        match (&self.get_subspace(), &out.get_subspace()) {
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

        self.get_path()
            .relative_encode(&out.get_path(), consumer)
            .await?;

        encode_compact_width_be(start_diff, consumer).await?;

        if self.get_times().end != RangeEnd::Open {
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
    /// Decode an [`Area`] relative to another [`Area`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_decode<Producer>(
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
            let id = S::decode(producer).await?;
            let sub = AreaSubspace::Id(id);

            // === Necessary to produce canonic encodings. ===
            // Verify that subspace wasn't needlessly encoded
            if &sub == out.get_subspace() {
                return Err(DecodeError::InvalidInput);
            }
            // ===============================================

            sub
        } else {
            out.get_subspace().clone()
        };

        // Verify that the decoded subspace is included by the reference subspace
        match (&out.get_subspace(), &subspace) {
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

        let path = Path::relative_decode(&out.get_path(), producer).await?;

        // Verify the decoded path is prefixed by the reference path
        if !path.is_prefixed_by(&out.get_path()) {
            return Err(DecodeError::InvalidInput);
        }

        let start_diff = decode_compact_width_be(start_diff_compact_width, producer).await?;

        let start = if add_start_diff {
            out.get_times().start.checked_add(start_diff)
        } else {
            u64::from(&out.get_times().end).checked_sub(start_diff)
        }
        .ok_or(DecodeError::InvalidInput)?;

        // Verify they sent correct start diff
        let expected_start_diff = core::cmp::min(
            start.checked_sub(out.get_times().start),
            u64::from(&out.get_times().end).checked_sub(start),
        )
        .ok_or(DecodeError::InvalidInput)?;

        if expected_start_diff != start_diff {
            return Err(DecodeError::InvalidInput);
        }

        // === Necessary to produce canonic encodings. ===
        // Verify that bit 2 of the header was set correctly
        let should_add_start_diff = start_diff
            == start
                .checked_sub(out.get_times().start)
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
                out.get_times().start.checked_add(end_diff)
            } else {
                u64::from(&out.get_times().end).checked_sub(end_diff)
            }
            .ok_or(DecodeError::InvalidInput)?;

            // Verify they sent correct end diff
            let expected_end_diff = core::cmp::min(
                end.checked_sub(out.get_times().start),
                u64::from(&out.get_times().end).checked_sub(end),
            )
            .ok_or(DecodeError::InvalidInput)?;

            if end_diff != expected_end_diff {
                return Err(DecodeError::InvalidInput);
            }

            // === Necessary to produce canonic encodings. ===
            let should_add_end_diff = end_diff
                == end
                    .checked_sub(out.get_times().start)
                    .ok_or(DecodeError::InvalidInput)?;

            if add_end_diff != should_add_end_diff {
                return Err(DecodeError::InvalidInput);
            }
            // ============================================

            RangeEnd::Closed(end)
        };

        let times = Range { start, end };

        // Verify the decoded time range is included by the reference time range
        if !out.get_times().includes_range(&times) {
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
    /// Encode an [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
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
        let start_to_start = self.get_times().start.abs_diff(reference.get_times().start);
        let start_to_end = match reference.get_times().end {
            RangeEnd::Closed(end) => self.get_times().start.abs_diff(end),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_start = match self.get_times().end {
            RangeEnd::Closed(end) => end.abs_diff(reference.get_times().start),
            RangeEnd::Open => u64::MAX,
        };
        let end_to_end = match (&self.get_times().end, &reference.get_times().end) {
            (RangeEnd::Closed(self_end), RangeEnd::Closed(ref_end)) => self_end.abs_diff(*ref_end),
            (RangeEnd::Closed(_), RangeEnd::Open) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Closed(_)) => u64::MAX,
            (RangeEnd::Open, RangeEnd::Open) => 0, // shouldn't matter right???
        };

        let start_time_diff = core::cmp::min(start_to_start, start_to_end);

        let end_time_diff = core::cmp::min(end_to_start, end_to_end);

        let mut header_1 = 0b0000_0000;

        // Bits 0, 1 - Encode r.get_subspaces().start?
        if self.get_subspaces().start == reference.get_subspaces().start {
            header_1 |= 0b0100_0000;
        } else if reference.get_subspaces().end == self.get_subspaces().start {
            header_1 |= 0b1000_0000;
        } else {
            header_1 |= 0b1100_0000;
        }

        // Bits 2, 3 - Encode r.get_subspaces().end?
        if self.get_subspaces().end == RangeEnd::Open {
            // Do nothing
        } else if self.get_subspaces().end == reference.get_subspaces().start {
            header_1 |= 0b0001_0000;
        } else if self.get_subspaces().end == reference.get_subspaces().end {
            header_1 |= 0b0010_0000;
        } else if self.get_subspaces().end != RangeEnd::Open {
            header_1 |= 0b0011_0000;
        }

        // Bit 4 - Encode r.get_paths().start relative to ref.get_paths().start or to ref.get_paths().end?
        if let RangeEnd::Closed(ref_path_end) = &reference.get_paths().end {
            let lcp_start_start = self
                .get_paths()
                .start
                .longest_common_prefix(&reference.get_paths().start);
            let lcp_start_end = self.get_paths().start.longest_common_prefix(ref_path_end);

            if lcp_start_start.get_component_count() >= lcp_start_end.get_component_count() {
                header_1 |= 0b0000_1000;
            }
        } else {
            header_1 |= 0b0000_1000;
        }

        // Bit 5 - Self path end open?
        if self.get_paths().end == RangeEnd::Open {
            header_1 |= 0b0000_0100;
        }

        // Bit 6 - Encode r.get_paths().end relative to ref.get_paths().start or to ref.get_paths().end (if at all)?
        match (&self.get_paths().end, &reference.get_paths().end) {
            (RangeEnd::Closed(self_path_end), RangeEnd::Closed(ref_path_end)) => {
                let lcp_end_start =
                    self_path_end.longest_common_prefix(&reference.get_paths().start);
                let lcp_end_end = self_path_end.longest_common_prefix(ref_path_end);

                if lcp_end_start.get_component_count() > lcp_end_end.get_component_count() {
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
        if self.get_times().end == RangeEnd::Open {
            header_1 |= 0b0000_0001;
        }

        consumer.consume(header_1).await?;

        let mut header_2 = 0b0000_0000;

        // Bit 8 - Encode r.get_times().start relative to ref.get_times().start or ref.get_times().end?
        if start_to_start <= start_to_end {
            header_2 |= 0b1000_0000;
        }

        // Bit 9 -Add or subtract start_time_diff?
        if is_bitflagged(header_2, 0) && self.get_times().start >= reference.get_times().start
            || !is_bitflagged(header_2, 0) && self.get_times().start >= reference.get_times().end
        {
            header_2 |= 0b0100_0000;
        }

        // Bit 10, 11 - 2-bit integer n such that 2^n gives compact_width(start_time_diff)
        header_2 |= CompactWidth::from_u64(start_time_diff).bitmask(2);

        // Bit 12 - Encode r.get_times().end relative to ref.get_times().start or ref.get_times().end (if at all)?
        if self.get_times().end != RangeEnd::Open && end_to_start <= end_to_end {
            header_2 |= 0b0000_1000;
        }

        // Bit 13 - Add or subtract end_time_diff (if encoding it at all)?
        if self.get_times().end == RangeEnd::Open {
            // do nothing
        } else if (is_bitflagged(header_2, 4)
            && self.get_times().end >= reference.get_times().start)
            || (!is_bitflagged(header_2, 4) && self.get_times().end >= reference.get_times().end)
        {
            header_2 |= 0b0000_0100;
        }

        // Bits 14, 15 - ignored, or 2-bit integer n such that 2^n gives compact_width(end_time_diff)
        if self.get_times().end == RangeEnd::Open {
            // do nothing
        } else {
            header_2 |= CompactWidth::from_u64(end_time_diff).bitmask(6);
        }

        consumer.consume(header_2).await?;

        if (self.get_subspaces().start == reference.get_subspaces().start)
            || (reference.get_subspaces().end == self.get_subspaces().start)
        {
            // Don't encode
        } else {
            self.get_subspaces().start.encode(consumer).await?;
        }

        if self.get_subspaces().end == RangeEnd::Open
            || (self.get_subspaces().end == reference.get_subspaces().start)
            || (self.get_subspaces().end == reference.get_subspaces().end)
        {
            // Don't encode end subspace
        } else if let RangeEnd::Closed(end_subspace) = &self.get_subspaces().end {
            end_subspace.encode(consumer).await?;
        }

        if is_bitflagged(header_1, 4) {
            self.get_paths()
                .start
                .relative_encode(&reference.get_paths().start, consumer)
                .await?;
        } else if let RangeEnd::Closed(end_path) = &reference.get_paths().end {
            self.get_paths()
                .start
                .relative_encode(end_path, consumer)
                .await?;
        }

        if let RangeEnd::Closed(end_path) = &self.get_paths().end {
            if is_bitflagged(header_1, 6) {
                end_path
                    .relative_encode(&reference.get_paths().start, consumer)
                    .await?
            } else if let RangeEnd::Closed(ref_end_path) = &reference.get_paths().end {
                end_path.relative_encode(ref_end_path, consumer).await?;
            }
        }

        encode_compact_width_be(start_time_diff, consumer).await?;
        encode_compact_width_be(end_time_diff, consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Range3d<MCL, MCC, MPL, S>> for Range3d<MCL, MCC, MPL, S>
where
    S: SubspaceId + Decodable + std::fmt::Debug,
{
    /// Encode an [`Range3d`] relative to another [`Range3d`] which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#area_include_area) it.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_area_in_area).
    async fn relative_decode<Producer>(
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
        let start_time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header_2, 2);
        let is_times_end_rel_to_start = is_bitflagged(header_2, 4);
        let add_or_subtract_end_time_diff = is_bitflagged(header_2, 5);
        let end_time_diff_compact_width = CompactWidth::decode_fixed_width_bitmask(header_2, 6);

        // Decode subspace start
        let subspace_start = match subspace_start_flags {
            0b0100_0000 => reference.get_subspaces().start.clone(),
            0b1000_0000 => match &reference.get_subspaces().end {
                RangeEnd::Closed(end) => end.clone(),
                RangeEnd::Open => Err(DecodeError::InvalidInput)?,
            },
            // This can only be 0b1100_0000
            _ => S::decode(producer).await?,
        };

        let subspace_end = match subspace_end_flags {
            0b0000_0000 => RangeEnd::Open,
            0b0001_0000 => RangeEnd::Closed(reference.get_subspaces().start.clone()),
            0b0010_0000 => match &reference.get_subspaces().end {
                RangeEnd::Closed(end) => RangeEnd::Closed(end.clone()),
                RangeEnd::Open => Err(DecodeError::InvalidInput)?,
            },
            // This can only be 0b0011_0000
            _ => RangeEnd::Closed(S::decode(producer).await?),
        };

        let path_start = match (is_path_start_rel_to_start, &reference.get_paths().end) {
            (true, RangeEnd::Closed(_)) => {
                Path::relative_decode(&reference.get_paths().start, producer).await?
            }
            (true, RangeEnd::Open) => {
                Path::relative_decode(&reference.get_paths().start, producer).await?
            }
            (false, RangeEnd::Closed(path_end)) => {
                Path::relative_decode(path_end, producer).await?
            }
            (false, RangeEnd::Open) => Err(DecodeError::InvalidInput)?,
        };

        let path_end = if is_path_end_open {
            RangeEnd::Open
        } else if is_path_end_rel_to_start {
            RangeEnd::Closed(Path::relative_decode(&reference.get_paths().start, producer).await?)
        } else {
            match &reference.get_paths().end {
                RangeEnd::Closed(end) => {
                    RangeEnd::Closed(Path::relative_decode(end, producer).await?)
                }
                RangeEnd::Open => Err(DecodeError::InvalidInput)?,
            }
        };

        let start_time_diff =
            decode_compact_width_be(start_time_diff_compact_width, producer).await?;

        let time_start = match (is_time_start_rel_to_start, add_or_subtract_start_time_diff) {
            (true, true) => reference.get_times().start.checked_add(start_time_diff),
            (true, false) => reference.get_times().start.checked_sub(start_time_diff),
            (false, true) => match reference.get_times().end {
                RangeEnd::Closed(ref_end) => ref_end.checked_add(start_time_diff),
                RangeEnd::Open => Err(DecodeError::InvalidInput)?,
            },
            (false, false) => match reference.get_times().end {
                RangeEnd::Closed(ref_end) => ref_end.checked_sub(start_time_diff),
                RangeEnd::Open => Err(DecodeError::InvalidInput)?,
            },
        }
        .ok_or(DecodeError::InvalidInput)?;

        let end_time_diff = decode_compact_width_be(end_time_diff_compact_width, producer).await?;

        let time_end = if is_times_end_open {
            RangeEnd::Open
        } else {
            match (is_times_end_rel_to_start, add_or_subtract_end_time_diff) {
                (true, true) => RangeEnd::Closed(
                    reference
                        .get_times()
                        .start
                        .checked_add(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,
                ),
                (true, false) => RangeEnd::Closed(
                    reference
                        .get_times()
                        .start
                        .checked_sub(end_time_diff)
                        .ok_or(DecodeError::InvalidInput)?,
                ),
                (false, true) => match reference.get_times().end {
                    RangeEnd::Closed(ref_end) => RangeEnd::Closed(
                        ref_end
                            .checked_add(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,
                    ),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
                (false, false) => match reference.get_times().end {
                    RangeEnd::Closed(ref_end) => RangeEnd::Closed(
                        ref_end
                            .checked_sub(end_time_diff)
                            .ok_or(DecodeError::InvalidInput)?,
                    ),
                    RangeEnd::Open => Err(DecodeError::InvalidInput)?,
                },
            }
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
