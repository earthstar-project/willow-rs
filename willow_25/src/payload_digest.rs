use ufotofu::BulkProducer;
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};

use willow_data_model::PayloadDigest;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use willow_sideload::SideloadPayloadDigest;

/// A [BLAKE3](https://en.wikipedia.org/wiki/BLAKE_(hash_function)#BLAKE3) hash digest suitable for the Willow Data Model's [`PayloadDigest`](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest) parameter.
///
/// Note: this will eventually be replaced by WILLIAM3 hash digest. Contact mail@aljoscha-meyer.de for details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadDigest25(blake3::Hash);

impl PayloadDigest25 {
    /** Returns a new [`PayloadDigest25`] created from a slice of bytes. */
    pub fn new_from_slice(slice: &[u8]) -> Self {
        let mut hasher = Self::hasher();
        Self::write(&mut hasher, slice);
        Self(hasher.finalize())
    }

    /** Returns a new [`PayloadDigest25`] created from all the bytes produced by a [`ufotofu::BulkProducer`]. */
    pub async fn new_from_producer<P: BulkProducer<Item = u8>>(
        producer: &mut P,
    ) -> Result<Self, P::Error> {
        let mut hasher = Self::hasher();

        loop {
            match producer.expose_items().await? {
                either::Left(items) => {
                    let len = items.len();
                    Self::write(&mut hasher, items);
                    producer.consider_produced(len).await?;
                }
                either::Right(_fin) => return Ok(Self(hasher.finalize())),
            }
        }
    }
}

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

impl SideloadPayloadDigest for PayloadDigest25 {
    fn default_payload_digest() -> Self {
        Self(blake3::Hash::from_bytes([0; 32]))
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
