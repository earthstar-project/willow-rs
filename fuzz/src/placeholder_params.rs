use arbitrary::Arbitrary;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{Decodable, DecodableCanonic, DecodeError, Encodable, RelationDecodable};
use willow_data_model::PayloadDigest;

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FakePayloadDigest([u8; 32]);

impl Encodable for FakePayloadDigest {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        consumer
            .bulk_consume_full_slice(&self.0)
            .await
            .map_err(|f| f.reason)?;

        Ok(())
    }
}

impl Decodable for FakePayloadDigest {
    type ErrorReason = DecodingWentWrong;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice = [0u8; 32];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        Ok(FakePayloadDigest(slice))
    }
}

impl DecodableCanonic for FakePayloadDigest {
    type ErrorCanonic = DecodingWentWrong;

    async fn decode_canonic<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
    where
        P: BulkProducer<Item = u8>,
    {
        Self::decode(producer);
    }
}

impl PayloadDigest for FakePayloadDigest {}
