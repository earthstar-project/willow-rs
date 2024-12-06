// Path <> Path

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    DecodeError, DecodingWentWrong, RelativeDecodable, RelativeDecodableCanonic,
    RelativeDecodableSync, RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use crate::Path;

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeEncodable<Path<MCL, MCC, MPL>>
    for Path<MCL, MCC, MPL>
{
    /// Encodes this [`Path`] relative to a reference [`Path`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_encode<Consumer>(
        &self,
        consumer: &mut Consumer,
        reference: &Path<MCL, MCC, MPL>,
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        /*
        let lcp = self.longest_common_prefix(reference);
        let lcp_component_count = lcp.component_count();
        encode_max_power(lcp_component_count, MCC, consumer).await?;

        let suffix_component_count = self.component_count() - lcp_component_count;
        encode_max_power(suffix_component_count, MCC, consumer).await?;

        for component in self.suffix_components(lcp_component_count) {
            encode_max_power(component.len(), MCL, consumer).await?;

            consumer
                .bulk_consume_full_slice(component.as_ref())
                .await
                .map_err(|f| f.reason)?;
        }

        Ok(())
        */

        todo!();
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodable<Path<MCL, MCC, MPL>, DecodingWentWrong> for Path<MCL, MCC, MPL>
{
    /// Decodes a [`Path`] relative to a reference [`Path`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Path<MCL, MCC, MPL>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        /*
                    let lcp_component_count: usize = decode_max_power(MCC, producer).await?.try_into()?;

                    if lcp_component_count == 0 {
                        let decoded = Path::<MCL, MCC, MPL>::decode_canonical(producer).await?;

                        // === Necessary to produce canonic encodings. ===
                        if lcp_component_count != decoded.longest_common_prefix(reference).component_count()
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
                    let raw_prefix_acc_component_lengths = &prefix.raw_buf()
                        [size_of::<usize>()..size_of::<usize>() * (lcp_component_count + 1)];
                    unsafe {
                        // Safe because len is less than size_of::<usize>() times the MCC, because `p
                        refix` respects the MCC.
                        buf.set_many_component_accumulated_lengths_from_ne(
                            raw_prefix_acc_component_lengths,
                        );

                        let mut accumulated_component_length: usize = prefix.path_length(); // Always holds the acc length of all components we copied so far.
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
                                        .bulk_overwrite_full_slice(unsafe {
                                            // Safe because we called set_component_Accumulated_length for all j <= i
                                            buf.path_data_as_mut(i)
                                        })
                                        .await?;
                    let decoded = unsafe { buf.to_path(total_component_count) };

                    // === Necessary to produce canonic encodings. ===
                    if lcp_component_count != decoded.longest_common_prefix(reference).component_count() {
                        return Err(DecodeError::InvalidInput);

                    Ok(decoded)
        */
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodableCanonic<Path<MCL, MCC, MPL>, DecodingWentWrong, DecodingWentWrong>
    for Path<MCL, MCC, MPL>
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Path<MCL, MCC, MPL>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, DecodingWentWrong>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncodableSync<Path<MCL, MCC, MPL>> for Path<MCL, MCC, MPL>
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodableSync<Path<MCL, MCC, MPL>, DecodingWentWrong> for Path<MCL, MCC, MPL>
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncodableKnownSize<Path<MCL, MCC, MPL>> for Path<MCL, MCC, MPL>
{
    fn relative_len_of_encoding(&self, r: &Path<MCL, MCC, MPL>) -> usize {
        todo!()
    }
}
