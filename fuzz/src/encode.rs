use ufotofu::{
    common::consumer::TestConsumer,
    local_nb::{
        producer::{FromBoxedSlice, FromSlice},
        BufferedConsumer, BulkConsumer,
    },
    sync::consumer::IntoVec,
};

use willow_data_model::encoding::{
    error::DecodeError,
    parameters::{Decodable, Encodable},
    relativity::{RelativeDecodable, RelativeEncodable},
};

pub async fn encoding_roundtrip<T, C>(item: T, consumer: &mut TestConsumer<u8, u16, ()>)
where
    T: Encodable + Decodable + std::fmt::Debug + PartialEq + Eq,
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
    let mut producer = FromBoxedSlice::from_vec(new_vec);

    // Check for correct errors
    let decoded_item = T::decode(&mut producer).await.unwrap();

    assert_eq!(decoded_item, item);
}

pub async fn encoding_random<T>(data: &[u8])
where
    T: Encodable + Decodable,
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

pub async fn relative_encoding_roundtrip<T, R, C>(
    subject: T,
    reference: R,
    consumer: &mut TestConsumer<u8, u16, ()>,
) where
    T: std::fmt::Debug + PartialEq + Eq + RelativeEncodable<R> + RelativeDecodable<R>,
    R: std::fmt::Debug,
    C: BulkConsumer<Item = u8>,
{
    // println!("item {:?}", subject);
    // println!("ref {:?}", reference);

    if let Err(_err) = subject.relative_encode(&reference, consumer).await {
        return;
    }

    if let Err(_err) = consumer.flush().await {
        return;
    }

    let mut new_vec = Vec::new();

    new_vec.extend_from_slice(consumer.as_ref());

    // THis should eventually be a testproducer, when we are able to initialise one with some known data.
    let mut producer = FromBoxedSlice::from_vec(new_vec);

    // Check for correct errors
    let decoded_item = T::relative_decode(&reference, &mut producer).await.unwrap();

    assert_eq!(decoded_item, subject);
}

pub async fn relative_encoding_random<R, T>(reference: R, data: &[u8])
where
    T: RelativeEncodable<R> + RelativeDecodable<R> + std::fmt::Debug,
    R: std::fmt::Debug,
{
    let mut producer = FromSlice::new(data);

    match T::relative_decode(&reference, &mut producer).await {
        Ok(item) => {
            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            //  println!("item {:?}", item);
            //  println!("ref {:?}", reference);

            item.relative_encode(&reference, &mut consumer)
                .await
                .unwrap();

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
