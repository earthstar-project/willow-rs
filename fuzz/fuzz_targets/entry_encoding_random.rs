#![no_main]

use arbitrary::Arbitrary;
use ufotofu::{
    common::consumer::IntoVec,
    local_nb::{producer::SliceProducer, BulkConsumer, BulkProducer},
};

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;

use willow_data_model::{
    encoding::{
        entry::{decode_entry, encode_entry},
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decoder, Encoder},
    },
    parameters::PayloadDigest,
    path::PathRc,
};

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

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        let mut producer = SliceProducer::new(data);

        match decode_entry::<EsNamespaceId, IdentityId, PathRc<3, 3, 3>, FakePayloadDigest, _>(
            &mut producer,
        )
        .await
        {
            Ok(path) => {
                // It decoded to a valid path! Gasp!
                // Can we turn it back into the same encoding?
                let mut consumer = IntoVec::<u8>::new();

                encode_entry(&path, &mut consumer).await.unwrap();

                let encoded = consumer.as_ref().as_slice();

                assert_eq!(encoded, &data[0..producer.get_offset()]);
            }
            Err(err) => match err {
                // There was an error.
                DecodeError::Producer(_) => panic!("Returned producer error, when whe shouldn't!"),
                DecodeError::InvalidInput => {
                    // GOOD.
                }
                DecodeError::U64DoesNotFitUsize => {
                    panic!("Returned u64DoesNotFitUsize error, when we shouldn't!")
                }
            },
        };
    });
});
