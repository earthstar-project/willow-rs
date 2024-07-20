#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_data_model::encoding::error::{DecodeError, EncodingConsumerError};
use willow_data_model::encoding::parameters::{Decodable, Encodable};
use willow_data_model::entry::Entry;
use willow_data_model::grouping::area::Area;
use willow_data_model::parameters::PayloadDigest;
use willow_data_model_fuzz::relative_encoding_roundtrip;

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

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Area<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry, area, mut consumer) = data;

    if !area.includes_entry(&entry) {
        return;
    }

    let namespace = entry.namespace_id.clone();

    smol::block_on(async {
        println!("{:?}", entry);
        println!("{:?}", area);

        relative_encoding_roundtrip::<
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
            (EsNamespaceId, Area<16, 16, 16, IdentityId>),
            TestConsumer<u8, u16, ()>,
        >(entry, (namespace, area), &mut consumer)
        .await;
    });
});
