#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use ufotofu::local_nb::producer::FromVec;
use ufotofu::local_nb::{BufferedConsumer, BulkConsumer, BulkProducer};
use willow_data_model::encoding::entry::{decode_entry, encode_entry};
use willow_data_model::encoding::error::{DecodeError, EncodingConsumerError};
use willow_data_model::encoding::parameters::{Decoder, Encoder};
use willow_data_model::entry::Entry;
use willow_data_model::parameters::PayloadDigest;
use willow_data_model::path::PathRc;

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FakePayloadDigest([u8; 32]);

impl<C> Encoder<C> for FakePayloadDigest
where
    C: BulkConsumer<Item = u8>,
{
    async fn encode(&self, consumer: &mut C) -> Result<(), EncodingConsumerError<C::Error>> {
        consumer.bulk_consume_full_slice(&self.0).await?;

        Ok(())
    }
}

impl<P> Decoder<P> for FakePayloadDigest
where
    P: BulkProducer<Item = u8>,
{
    async fn decode(producer: &mut P) -> Result<Self, DecodeError<P::Error>> {
        let mut slice = [0u8; 32];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        Ok(FakePayloadDigest(slice))
    }
}

impl PayloadDigest for FakePayloadDigest {}

fuzz_target!(|data: (
    Entry<EsNamespaceId, IdentityId, PathRc<3, 3, 3>, FakePayloadDigest>,
    TestConsumer<u8, u8, u8>
)| {
    let (entry, mut consumer) = data;

    smol::block_on(async {
        let consumer_should_error = consumer.should_error();

        if let Err(_err) = encode_entry(&entry, &mut consumer).await {
            assert!(consumer_should_error);
            return;
        }

        if let Err(_err) = consumer.flush().await {
            assert!(consumer_should_error);
            return;
        }

        let mut new_vec = Vec::new();

        new_vec.extend_from_slice(consumer.as_ref());

        // THis should eventually be a testproducer, when we are able to initialise one with some known data.
        let mut producer = FromVec::new(new_vec);

        // Check for correct errors
        let decoded_entry = decode_entry(&mut producer).await.unwrap();

        assert_eq!(decoded_entry, entry);
    });
});
