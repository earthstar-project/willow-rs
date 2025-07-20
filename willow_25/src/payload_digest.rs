use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::PayloadDigest;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

/// A [BLAKE3](https://en.wikipedia.org/wiki/BLAKE_(hash_function)#BLAKE3) hash digest suitable for the Willow Data Model's [`PayloadDigest`](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest) parameter.
///
/// Note: this will eventually be replaced by WILLIAM3 hash digest. Contact mail@aljoscha-meyer.de for details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadDigest25(blake3::Hash);

impl Default for PayloadDigest25 {
    fn default() -> Self {
        Self::finish(&Self::hasher())
    }
}

impl PartialOrd for PayloadDigest25 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PayloadDigest25 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl PayloadDigest for PayloadDigest25 {
    type Hasher = blake3::Hasher;

    fn hasher() -> Self::Hasher {
        blake3::Hasher::new()
    }

    fn finish(hasher: &Self::Hasher) -> Self {
        Self(hasher.finalize())
    }

    fn write(hasher: &mut Self::Hasher, bytes: &[u8]) {
        hasher.update(bytes);
    }
}

impl Encodable for PayloadDigest25 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer
            .consume_full_slice(self.0.as_bytes())
            .await
            .map_err(|err| err.reason)
    }
}

impl EncodableKnownSize for PayloadDigest25 {
    fn len_of_encoding(&self) -> usize {
        blake3::OUT_LEN
    }
}

impl EncodableSync for PayloadDigest25 {}

impl Decodable for PayloadDigest25 {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut out_slice: [u8; blake3::OUT_LEN] = [0; blake3::OUT_LEN];

        producer.bulk_overwrite_full_slice(&mut out_slice).await?;

        Ok(Self(blake3::Hash::from_bytes(out_slice)))
    }
}

impl DecodableCanonic for PayloadDigest25 {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut out_slice: [u8; blake3::OUT_LEN] = [0; blake3::OUT_LEN];

        producer.bulk_overwrite_full_slice(&mut out_slice).await?;

        Ok(Self(blake3::Hash::from_bytes(out_slice)))
    }
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for PayloadDigest25 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes: [u8; blake3::OUT_LEN] = Arbitrary::arbitrary(u)?;

        Ok(Self(blake3::Hash::from_bytes(bytes)))
    }
}
