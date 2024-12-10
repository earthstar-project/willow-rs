use ufotofu::{BulkConsumer, BulkProducer};

use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync, RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync,
    RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};

use compact_u64::*;

use super::*;

/// Essentially how to encode a Path, but working with an arbitrary iterator of components. Path encoding consists of calling this directly, relative path encoding consists of first encoding the length of the greatest common suffix and *then* calling this.
async fn encode_from_iterator_of_components<'a, const MCL: usize, C, I>(
    consumer: &mut C,
    path_length: u64,
    component_count: u64,
    components: I,
) -> Result<(), C::Error>
where
    C: BulkConsumer<Item = u8>,
    I: Iterator<Item = Component<'a, MCL>>,
{
    // First byte contains two 4-bit tags of `CompactU64`s:
    // total path length in bytes: 4 bit tag at offset zero
    // total number of components: 4 bit tag at offset four
    let path_length_tag = Tag::min_tag(path_length, TagWidth::four());
    let component_count_tag = Tag::min_tag(component_count, TagWidth::four());

    let first_byte = path_length_tag.data_at_offset(0) | component_count_tag.data_at_offset(4);
    consumer.consume(first_byte).await?;

    // Next, encode the total path length in compact bytes.
    let total_bytes_bytes = CompactU64(path_length);
    total_bytes_bytes
        .relative_encode(consumer, &path_length_tag.encoding_width())
        .await?;

    // Next, encode the  total number of components in compact bytes.
    let component_count_bytes = CompactU64(component_count);
    component_count_bytes
        .relative_encode(consumer, &component_count_tag.encoding_width())
        .await?;

    // Then, encode the components. Each is prefixed by its lenght as an 8-bit-tag CompactU64, except for the final component.
    for (i, component) in components.enumerate() {
        // The length of the final component is omitted (because a decoder can infer it from the total length and all prior components' lengths).
        if i as u64 + 1 != component_count {
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

impl<const MCL: usize, const MCC: usize, const MPL: usize> Encodable for Path<MCL, MCC, MPL> {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        encode_from_iterator_of_components::<MCL, _, _>(
            consumer,
            self.path_length() as u64,
            self.component_count() as u64,
            self.components(),
        )
        .await
    }
}

// Decodes the path length and component count as expected at the start of a path encoding, generic over whether the encoding must be canonic or not.
// Implemented as a dedicated function so that it can be used in both absolute and relative decoding.
async fn decode_total_length_and_component_count_maybe_canonic<const CANONIC: bool, P>(
    producer: &mut P,
) -> Result<(usize, usize), DecodeError<P::Final, P::Error, Blame>>
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
    let total_length = Blame::u64_to_usize(total_length)?;
    let component_count = Blame::u64_to_usize(component_count)?;

    Ok((total_length, component_count))
}

// Decodes the components of a path encoding, generic over whether the encoding must be canonic or not. Appends them into a PathBuilder. Needs to know the total length of components that had already been appended to that PathBuilder before.
// Implemented as a dedicated function so that it can be used in both absolute and relative decoding.
async fn decode_components_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    P,
>(
    producer: &mut P,
    mut builder: PathBuilder<MCL, MCC, MPL>,
    initial_accumulated_component_length: usize,
    remaining_component_count: usize,
    expected_total_length: usize,
) -> Result<Path<MCL, MCC, MPL>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
{
    // Now decode the actual components.
    // We track the sum of the lengths of all decoded components so far, because we need it to determine the length of the final component.
    let mut accumulated_component_length = initial_accumulated_component_length;

    // Handle decoding of the empty path with dedicated logic to prevent underflows in loop counters =S
    if remaining_component_count == 0 {
        if expected_total_length > accumulated_component_length {
            // Claimed length is incorrect
            return Err(DecodeError::Other(Blame::TheirFault));
        } else {
            // Nothing more to do, decoding an empty path turns out to be simple!
            return Ok(builder.build());
        }
    } else {
        // We have at least one component.

        // Decode all but the final one (because the final one is encoded without its lenght and hence requires dedicated logic to decode).
        for _ in 1..remaining_component_count {
            let component_len = Blame::u64_to_usize(decode_cu64::<CANONIC, _>(producer).await?)?;

            if component_len > MCL {
                // Decoded path must respect the MCL.
                return Err(DecodeError::Other(Blame::TheirFault));
            } else {
                // Increase the accumulated length, accounting for errors.
                accumulated_component_length = accumulated_component_length
                    .checked_add(component_len)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?;

                // Copy the component bytes into the Path.
                builder
                    .append_component_from_bulk_producer(component_len, producer)
                    .await?;
            }
        }

        // For the final component, compute its length. If the computation result would be negative, then the encoding was invalid.
        let final_component_length = expected_total_length
            .checked_sub(accumulated_component_length)
            .ok_or(DecodeError::Other(Blame::TheirFault))?;

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
    let (total_length, component_count) =
        decode_total_length_and_component_count_maybe_canonic::<CANONIC, _>(producer).await?;

    // Preallocate all storage for the path. Error if the total length or component_count are greater than MPL and MCC respectively allow.
    let builder = PathBuilder::new(total_length, component_count)
        .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

    decode_components_maybe_canonic::<CANONIC, MCL, MCC, MPL, _>(
        producer,
        builder,
        0,
        component_count,
        total_length,
    )
    .await
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

// Separate function to allow for reuse in relative encoding.
fn encoding_len_from_iterator_of_components<'a, const MCL: usize, I>(
    path_length: u64,
    component_count: usize,
    components: I,
) -> usize
where
    I: Iterator<Item = Component<'a, MCL>>,
{
    let mut total_enc_len = 1; // First byte for the two four-bit tags at the start of the encoding.

    total_enc_len += EncodingWidth::min_width(path_length, TagWidth::four()).as_usize();
    total_enc_len += EncodingWidth::min_width(component_count as u64, TagWidth::four()).as_usize();

    for (i, comp) in components.enumerate() {
        if i + 1 < component_count {
            total_enc_len += CompactU64(comp.len() as u64).len_of_encoding();
        }

        total_enc_len += comp.len();
    }

    total_enc_len
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> EncodableKnownSize
    for Path<MCL, MCC, MPL>
{
    fn len_of_encoding(&self) -> usize {
        encoding_len_from_iterator_of_components::<MCL, _>(
            self.path_length() as u64,
            self.component_count(),
            self.components(),
        )
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> EncodableSync for Path<MCL, MCC, MPL> {}
impl<const MCL: usize, const MCC: usize, const MPL: usize> DecodableSync for Path<MCL, MCC, MPL> {}

// Relative encoding path <> path

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
        let lcp = self.longest_common_prefix(reference);

        CompactU64(lcp.component_count() as u64)
            .encode(consumer)
            .await?;

        let suffix_length = self.path_length() - lcp.path_length();
        let suffix_component_count = self.component_count() - lcp.component_count();

        encode_from_iterator_of_components::<MCL, _, _>(
            consumer,
            suffix_length as u64,
            suffix_component_count as u64,
            self.suffix_components(lcp.component_count()),
        )
        .await
    }
}

// Decodes a path relative to another path, generic over whether the encoding must be canonic or not.
async fn relative_decode_maybe_canonic<
    const CANONIC: bool,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    P,
>(
    producer: &mut P,
    r: &Path<MCL, MCC, MPL>,
) -> Result<Path<MCL, MCC, MPL>, DecodeError<P::Final, P::Error, Blame>>
where
    P: BulkProducer<Item = u8>,
{
    let prefix_component_count = Blame::u64_to_usize(decode_cu64::<CANONIC, _>(producer).await?)?;

    let (suffix_length, suffix_component_count) =
        decode_total_length_and_component_count_maybe_canonic::<CANONIC, _>(producer).await?;

    let prefix = r
        .create_prefix(prefix_component_count)
        .ok_or(DecodeError::Other(Blame::TheirFault))?;

    let total_length = prefix
        .path_length()
        .checked_add(suffix_length)
        .ok_or(DecodeError::Other(Blame::TheirFault))?;
    let total_component_count = prefix_component_count
        .checked_add(suffix_component_count)
        .ok_or(DecodeError::Other(Blame::TheirFault))?;

    // Preallocate all storage for the path. Error if the total length or component_count are greater than MPL and MCC respectively allow.
    let builder = PathBuilder::new_from_prefix(
        total_length,
        total_component_count,
        &prefix,
        prefix.component_count(),
    )
    .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

    // Decode the remaining components, add them to the builder, then build.
    let decoded = decode_components_maybe_canonic::<CANONIC, MCL, MCC, MPL, _>(
        producer,
        builder,
        prefix.path_length(),
        suffix_component_count,
        total_length,
    )
    .await?;

    if CANONIC {
        // Did the encoding use the *longest* common prefix?
        if prefix.component_count() == r.component_count() {
            // Could not have taken a longer prefix of `r`, i.e., the prefix was maximal.
            Ok(decoded)
        } else if prefix.component_count() == decoded.component_count() {
            // The prefix was the full path to decode, so it clearly was chosen maximally.
            Ok(decoded)
        } else {
            // We check whether the next-longer prefix of `r` could have also been used for encoding. If so, error.
            // To efficiently check, we check whether the next component of `r` is equal to its counterpart in what we decoded.
            // Both next components exist, otherwise we would have been in an earlier branch of the `if` expression.
            if r.component(prefix.component_count()).unwrap()
                == decoded.component(prefix.component_count()).unwrap()
            {
                // Could have used a longer prefix for decoding. Not canonic!
                Err(DecodeError::Other(Blame::TheirFault))
            } else {
                // Encoding was minimal, yay =)
                Ok(decoded)
            }
        }
    } else {
        // No additional canonicity checks needed.
        Ok(decoded)
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodable<Path<MCL, MCC, MPL>, Blame> for Path<MCL, MCC, MPL>
{
    /// Decodes a [`Path`] relative to a reference [`Path`].
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Path<MCL, MCC, MPL>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<false, MCL, MCC, MPL, _>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodableCanonic<Path<MCL, MCC, MPL>, Blame, Blame> for Path<MCL, MCC, MPL>
{
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Path<MCL, MCC, MPL>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        relative_decode_maybe_canonic::<true, MCL, MCC, MPL, _>(producer, r).await
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncodableKnownSize<Path<MCL, MCC, MPL>> for Path<MCL, MCC, MPL>
{
    fn relative_len_of_encoding(&self, r: &Path<MCL, MCC, MPL>) -> usize {
        let lcp = self.longest_common_prefix(r);
        let path_len_of_suffix = self.path_length() - lcp.path_length();
        let component_count_of_suffix = self.component_count() - lcp.component_count();

        let mut total_enc_len = 0;

        // Number of components in the longest common prefix, encoded as a CompactU64 with an 8-bit tag.
        total_enc_len += CompactU64(lcp.component_count() as u64).len_of_encoding();

        total_enc_len += encoding_len_from_iterator_of_components::<MCL, _>(
            path_len_of_suffix as u64,
            component_count_of_suffix,
            self.suffix_components(lcp.component_count()),
        );

        total_enc_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncodableSync<Path<MCL, MCC, MPL>> for Path<MCL, MCC, MPL>
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeDecodableSync<Path<MCL, MCC, MPL>, Blame> for Path<MCL, MCC, MPL>
{
}

// TODO move stuff below to more appropriate files/crates

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
