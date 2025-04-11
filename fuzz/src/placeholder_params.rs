use std::{
    future::Future,
    hash::{DefaultHasher, Hasher},
};

use arbitrary::Arbitrary;
use meadowcap::{McPublicUserKey, SillyPublicKey, SillySig};
use signature::Verifier;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync,
};
use ufotofu_codec_endian::U64BE;
use willow_data_model::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId};

async fn encode_bytes<const BYTES_LENGTH: usize, C>(
    bytes: &[u8; BYTES_LENGTH],
    consumer: &mut C,
) -> Result<(), C::Error>
where
    C: BulkConsumer<Item = u8>,
{
    consumer
        .bulk_consume_full_slice(bytes)
        .await
        .map_err(|f| f.reason)?;

    Ok(())
}

async fn decode_bytes<const BYTES_LENGTH: usize, P, ErrorReason>(
    producer: &mut P,
) -> Result<[u8; BYTES_LENGTH], DecodeError<P::Final, P::Error, ErrorReason>>
where
    P: BulkProducer<Item = u8>,
{
    let mut slice = [0u8; BYTES_LENGTH];

    producer.bulk_overwrite_full_slice(&mut slice).await?;

    Ok(slice)
}

// Namespace ID

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct FakeNamespaceId(SillyPublicKey);

impl Encodable for FakeNamespaceId {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.0.encode(consumer).await
    }
}

impl Decodable for FakeNamespaceId {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Ok(Self(SillyPublicKey::decode(producer).await?))
    }
}

impl DecodableCanonic for FakeNamespaceId {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer).await
    }
}

impl EncodableKnownSize for FakeNamespaceId {
    fn len_of_encoding(&self) -> usize {
        self.0.len_of_encoding()
    }
}

impl EncodableSync for FakeNamespaceId {}
impl DecodableSync for FakeNamespaceId {}

impl NamespaceId for FakeNamespaceId {}

impl Verifier<SillySig> for FakeNamespaceId {
    fn verify(&self, msg: &[u8], signature: &SillySig) -> Result<(), signature::Error> {
        self.0.verify(msg, signature)
    }
}

// Subspace ID

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct FakeSubspaceId(SillyPublicKey);

impl Encodable for FakeSubspaceId {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.0.encode(consumer).await
    }
}

impl Decodable for FakeSubspaceId {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Ok(Self(SillyPublicKey::decode(producer).await?))
    }
}

impl DecodableCanonic for FakeSubspaceId {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer).await
    }
}

impl EncodableKnownSize for FakeSubspaceId {
    fn len_of_encoding(&self) -> usize {
        self.0.len_of_encoding()
    }
}

impl SubspaceId for FakeSubspaceId {
    fn successor(&self) -> Option<Self> {
        self.0.successor().map(Self)
    }
}

impl EncodableSync for FakeSubspaceId {}

impl Verifier<SillySig> for FakeSubspaceId {
    fn verify(&self, msg: &[u8], signature: &SillySig) -> Result<(), signature::Error> {
        self.0.verify(msg, signature)
    }
}

impl McPublicUserKey<SillySig> for FakeSubspaceId {}

// Payload digest

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct FakePayloadDigest([u8; 32]);

impl Encodable for FakePayloadDigest {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        encode_bytes(&self.0, consumer).await
    }
}

impl Decodable for FakePayloadDigest {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let bytes = decode_bytes(producer).await?;

        Ok(FakePayloadDigest(bytes))
    }
}

impl DecodableCanonic for FakePayloadDigest {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer).await
    }
}

impl EncodableKnownSize for FakePayloadDigest {
    fn len_of_encoding(&self) -> usize {
        32
    }
}

impl EncodableSync for FakePayloadDigest {}

impl PayloadDigest for FakePayloadDigest {
    type Hasher = DefaultHasher;

    fn finish(hasher: &Self::Hasher) -> Self {
        let hashy_numbers = hasher.finish();
        let bytes = U64BE(hashy_numbers).sync_encode_into_vec();
        let mut arr = [0x0; 32];
        arr[..8].clone_from_slice(&bytes);
        Self(arr)
    }

    fn write(hasher: &mut Self::Hasher, bytes: &[u8]) {
        hasher.write(bytes)
    }

    fn hasher() -> Self::Hasher {
        DefaultHasher::new()
    }
}

#[derive(Clone, Debug, Arbitrary, PartialEq, Eq, Hash)]
pub struct FakeAuthorisationToken(SillySig);

impl AuthorisationToken<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>
    for FakeAuthorisationToken
{
    fn is_authorised_write(
        &self,
        entry: &willow_data_model::Entry<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
        >,
    ) -> bool {
        let message = entry.sync_encode_into_boxed_slice();

        entry.subspace_id().verify(&message, &self.0).is_ok()
    }
}

impl Encodable for FakeAuthorisationToken {
    fn encode<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.0.encode(consumer)
    }
}

impl Decodable for FakeAuthorisationToken {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        Ok(Self(SillySig::decode(producer).await?))
    }
}
