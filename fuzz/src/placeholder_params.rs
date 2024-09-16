use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use ufotofu::sync::{BulkConsumer as BulkConsumerSync, BulkProducer as BulkProducerSync};
use willow_data_model::PayloadDigest;
use willow_encoding::{
    sync::{
        Decodable as DecodableSync, Encodable as EncodableSync,
        RelationDecodable as RelationDecodableSync,
    },
    Decodable, DecodeError, Encodable, RelationDecodable,
};

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
    async fn decode_canonical<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
    where
        P: BulkProducer<Item = u8>,
    {
        let mut slice = [0u8; 32];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        Ok(FakePayloadDigest(slice))
    }
}

impl EncodableSync for FakePayloadDigest {
    fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumerSync<Item = u8>,
    {
        consumer
            .bulk_consume_full_slice(&self.0)
            .map_err(|f| f.reason)?;

        Ok(())
    }
}

impl DecodableSync for FakePayloadDigest {
    fn decode_canonical<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
    where
        P: BulkProducerSync<Item = u8>,
    {
        let mut slice = [0u8; 32];

        producer.bulk_overwrite_full_slice(&mut slice)?;

        Ok(FakePayloadDigest(slice))
    }
}

impl RelationDecodable for FakePayloadDigest {}

impl RelationDecodableSync for FakePayloadDigest {}

impl PayloadDigest for FakePayloadDigest {}
