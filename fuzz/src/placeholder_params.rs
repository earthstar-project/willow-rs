use arbitrary::Arbitrary;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::{NamespaceId, PayloadDigest, SubspaceId};

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

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FakeNamespaceId([u8; 16]);

impl Encodable for FakeNamespaceId {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        encode_bytes(&self.0, consumer).await
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
        let bytes = decode_bytes(producer).await?;

        Ok(FakeNamespaceId(bytes))
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
        16
    }
}

impl NamespaceId for FakeNamespaceId {}

// Subspace ID

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FakeSubspaceId([u8; 8]);

impl Encodable for FakeSubspaceId {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        encode_bytes(&self.0, consumer).await
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
        let bytes = decode_bytes(producer).await?;

        Ok(FakeSubspaceId(bytes))
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
        8
    }
}

impl SubspaceId for FakeSubspaceId {
    fn successor(&self) -> Option<Self> {
        // Wish we could avoid this allocation somehow.
        let mut new_id = self.0;

        for i in (0..self.0.len()).rev() {
            let byte = self.0.as_ref()[i];

            if byte == 255 {
                new_id[i] = 0;
            } else {
                new_id[i] = byte + 1;
                return Some(FakeSubspaceId(new_id));
            }
        }

        None
    }
}

// Payload digest

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
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

impl PayloadDigest for FakePayloadDigest {}
