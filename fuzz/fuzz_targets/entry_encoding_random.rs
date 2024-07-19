#![no_main]

use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;

use willow_data_model::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decodable, Encodable},
    },
    entry::Entry,
    parameters::PayloadDigest,
    path::PathRc,
};

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FakePayloadDigest([u8; 32]);

impl Encodable for FakePayloadDigest {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), EncodingConsumerError<C::Error>>
    where
        C: BulkConsumer<Item = u8>,
    {
        consumer.bulk_consume_full_slice(&self.0).await?;

        Ok(())
    }
}

impl Decodable for FakePayloadDigest {
    async fn decode<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
    where
        P: BulkProducer<Item = u8>,
    {
        let mut slice = [0u8; 32];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        Ok(FakePayloadDigest(slice))
    }
}

impl PayloadDigest for FakePayloadDigest {}

use libfuzzer_sys::fuzz_target;
use willow_data_model_fuzz::encoding_random;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<Entry<EsNamespaceId, IdentityId, PathRc<3, 3, 3>, FakePayloadDigest>>(
            data,
        )
        .await;
    });
});
