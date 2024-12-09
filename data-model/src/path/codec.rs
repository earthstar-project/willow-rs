use ufotofu::{BulkConsumer, BulkProducer};

use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync, RelativeDecodable, RelativeDecodableCanonic, RelativeEncodable,
};

use compact_u64::*;

use super::*;

impl<const MCL: usize, const MCC: usize, const MPL: usize> Encodable for Path<MCL, MCC, MPL> {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // First byte contains two 4-bit tags of `CompactU64`s:
        // total path length in bytes: 4 bit tag at offset zero
        // total number of components: 4 bit tag at offset four
        let path_length_tag = Tag::min_tag(self.path_length() as u64, TagWidth::four());
        let component_count_tag = Tag::min_tag(self.component_count() as u64, TagWidth::four());

        let first_byte = path_length_tag.data_at_offset(0) | component_count_tag.data_at_offset(4);
        consumer.consume(first_byte).await?;

        // Next, encode the total path length in compact bytes.
        let total_bytes_bytes = CompactU64(self.path_length() as u64);
        total_bytes_bytes
            .relative_encode(consumer, &path_length_tag.encoding_width())
            .await?;

        // Next, encode the  total number of components in compact bytes.
        let component_count_bytes = CompactU64(self.component_count() as u64);
        component_count_bytes
            .relative_encode(consumer, &component_count_tag.encoding_width())
            .await?;

        // Then, encode the components. Each is prefixed by its lenght as an 8-bit-tag CompactU64, except for the final component.
        for (i, component) in self.components().enumerate() {
            // The length of the final component is omitted (because a decoder can infer it from the total length and all prior components' lengths).
            if i != self.component_count() - 1 {
                CompactU64(component.len() as u64).encode(consumer).await?;
            }

            // Each component length (if any) is followed by the raw component data itself.
            consumer
                .bulk_consume_full_slice(component.as_ref())
                .await
                .map_err(|err| err.into_reason())?;
        }

        Ok(())
    }
}

// Decodes a path, generic over whether the encoding must be canonic or not.
async fn decode_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    P,
>(
    producer: &mut P,
) -> Result<Path<MCL, MCC, MPL>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
{
    // Decode the first byte - the two compact width tags for the path length and component count.
    let first_byte = producer.produce_item().await?;
    let path_length_tag = Tag::from_raw(first_byte, TagWidth::four(), 0);
    let component_count_tag = Tag::from_raw(first_byte, TagWidth::four(), 4);

    // Next, decode the total path length and the component count.
    let total_length = relative_decode_cu64::<CANONIC, _>(producer, &path_length_tag).await?;
    let component_count =
        relative_decode_cu64::<CANONIC, _>(producer, &component_count_tag).await?;

    // Convert them from u64 to usize, error if usize cannot represent the number.
    let total_length = u64_to_usize(total_length)?;
    let component_count = u64_to_usize(component_count)?;

    // Preallocate all storage for the path. Error if the total length or component_count are greater than MPL and MCC respectively allow.
    let mut builder = PathBuilder::new(total_length, component_count)
        .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

    // Now decode the actual components.
    // We track the sum of the lengths of all decoded components so far, because we need it to determine the length of the final component.
    let mut accumulated_component_length: usize = 0;

    // Handle decoding of the empty path with dedicated logic to prevent underflows in loop counters =S
    if component_count == 0 {
        if total_length > 0 {
            // Claimed length is incorrect
            return Err(DecodeError::Other(Blame::TheirFault));
        } else {
            // Nothing more to do, decoding an empty path turns out to be simple!
            return Ok(builder.build());
        }
    } else {
        // We have at least one component.

        // Decode all but the final one (because the final one is encoded without its lenght and hence requires dedicated logic to decode).
        for _ in 1..component_count {
            let component_len = u64_to_usize(decode_cu64::<CANONIC, _>(producer).await?)?;

            if component_len > MCL {
                // Decoded path must respect the MCL.
                return Err(DecodeError::Other(Blame::TheirFault));
            } else {
                // Increase the accumulated length, accounting for errors.
                match accumulated_component_length.checked_add(component_len) {
                    None => return Err(DecodeError::Other(Blame::TheirFault)),
                    Some(new_accumulated_length) => {
                        accumulated_component_length = new_accumulated_length;
                    }
                }

                // Copy the component bytes into the Path.
                builder
                    .append_component_from_bulk_producer(component_len, producer)
                    .await?;
            }
        }

        // For the final component, compute its length. If the computation result would be negative, then the encoding was invalid.
        match total_length.checked_sub(accumulated_component_length) {
            None => return Err(DecodeError::Other(Blame::TheirFault)),
            Some(final_component_length) => {
                if final_component_length > MCL {
                    // Decoded path must respect the MCL.
                    return Err(DecodeError::Other(Blame::TheirFault));
                } else {
                    // Copy the final component bytes into the Path.
                    builder
                        .append_component_from_bulk_producer(final_component_length, producer)
                        .await?;

                    // What a journey. We are done!
                    Ok(builder.build())
                }
            }
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Decodable for Path<MCL, MCC, MPL> {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        decode_maybe_canonic::<false, MCL, MCC, MPL, _>(producer).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> DecodableCanonic
    for Path<MCL, MCC, MPL>
{
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        decode_maybe_canonic::<true, MCL, MCC, MPL, _>(producer).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> EncodableKnownSize
    for Path<MCL, MCC, MPL>
{
    fn len_of_encoding(&self) -> usize {
        let mut total_enc_len = 1; // First byte for the two four-bit tags at the start of the encoding.

        total_enc_len +=
            EncodingWidth::min_width(self.path_length() as u64, TagWidth::four()).as_usize();
        total_enc_len +=
            EncodingWidth::min_width(self.component_count() as u64, TagWidth::four()).as_usize();

        for (i, comp) in self.components().enumerate() {
            if i + 1 < self.component_count() {
                total_enc_len += CompactU64(comp.len() as u64).len_of_encoding();
            }

            total_enc_len += comp.len();
        }

        total_enc_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> EncodableSync for Path<MCL, MCC, MPL> {}
impl<const MCL: usize, const MCC: usize, const MPL: usize> DecodableSync for Path<MCL, MCC, MPL> {}

// TODO move stuff below to more appropriate files/crates

/// Convert from a `u64` to a `usize`, reporting an error if the value does not fit into a `usize`.
pub fn u64_to_usize<F, E>(n: u64) -> Result<usize, DecodeError<F, E, Blame>> {
    usize::try_from(n).map_err(|_| DecodeError::Other(Blame::OurFault))
}

/// Decodes a `CompactU64` relative to a `Tag`, generic over whether the encoding must be canonic or not.
pub async fn relative_decode_cu64<const CANONIC: bool, P>(
    producer: &mut P,
    tag: &Tag,
) -> Result<u64, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
{
    if CANONIC {
        Ok(CompactU64::relative_decode_canonic(producer, tag)
            .await
            .map_err(|err| DecodeError::map_other(err, |_| Blame::TheirFault))?
            .0)
    } else {
        Ok(CompactU64::relative_decode(producer, tag)
            .await
            .map_err(|err| DecodeError::map_other(err, |_| Blame::TheirFault))?
            .0)
    }
}

/// Decodes a `CompactU64`, generic over whether the encoding must be canonic or not.
pub async fn decode_cu64<const CANONIC: bool, P>(
    producer: &mut P,
) -> Result<u64, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
{
    if CANONIC {
        Ok(CompactU64::decode_canonic(producer)
            .await
            .map_err(|err| DecodeError::map_other(err, |_| Blame::TheirFault))?
            .0)
    } else {
        Ok(CompactU64::decode(producer)
            .await
            .map_err(|err| DecodeError::map_other(err, |_| Blame::TheirFault))?
            .0)
    }
}
