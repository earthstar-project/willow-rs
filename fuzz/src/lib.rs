use ufotofu::{
    common::consumer::TestConsumer,
    local_nb::{
        producer::{FromSlice, FromVec},
        BufferedConsumer, BulkConsumer,
    },
    sync::consumer::IntoVec,
};
use willow_data_model::encoding::{
    error::DecodeError,
    parameters::{Decoder, Encoder},
};

pub async fn encoding_roundtrip<T, C>(item: T, consumer: &mut TestConsumer<u8, u16, ()>)
where
    T: Encoder + Decoder + std::fmt::Debug + PartialEq + Eq,
    C: BulkConsumer<Item = u8>,
{
    if let Err(_err) = item.encode(consumer).await {
        return;
    }

    if let Err(_err) = consumer.flush().await {
        return;
    }

    let mut new_vec = Vec::new();

    new_vec.extend_from_slice(consumer.as_ref());

    // THis should eventually be a testproducer, when we are able to initialise one with some known data.
    let mut producer = FromVec::new(new_vec);

    // Check for correct errors
    let decoded_path = T::decode(&mut producer).await.unwrap();

    assert_eq!(decoded_path, item);
}

pub async fn encoding_random<T>(data: &[u8])
where
    T: Encoder + Decoder,
{
    let mut producer = FromSlice::new(data);

    match T::decode(&mut producer).await {
        Ok(item) => {
            // It decoded to a valid path! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            item.encode(&mut consumer).await.unwrap();

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
}

pub mod path;
