use ufotofu::{
    common::consumer::TestConsumer,
    sync::consumer::IntoVec,
    sync::{
        producer::{FromBoxedSlice, FromSlice},
        BufferedConsumer, BulkConsumer,
    },
};

use willow_encoding::{
    sync::{Decodable, Encodable, RelativeDecodable, RelativeEncodable},
    DecodeError,
};

pub fn encoding_roundtrip<T, C>(item: T, consumer: &mut TestConsumer<u8, u16, ()>)
where
    T: Encodable + Decodable + std::fmt::Debug + PartialEq + Eq,
    C: BulkConsumer<Item = u8>,
{
    if let Err(_err) = item.encode(consumer) {
        return;
    }

    if let Err(_err) = consumer.flush() {
        return;
    }

    let mut new_vec = Vec::new();

    new_vec.extend_from_slice(consumer.consumed());

    // THis should eventually be a testproducer, when we are able to initialise one with some known data.
    let mut producer = FromBoxedSlice::from_vec(new_vec);

    // Check for correct errors
    let decoded_item = T::decode_canonical(&mut producer).unwrap();

    assert_eq!(decoded_item, item);
}

pub fn encoding_random<T>(data: &[u8])
where
    T: Encodable + Decodable + std::fmt::Debug,
{
    let mut producer = FromSlice::new(data);

    match T::decode_canonical(&mut producer) {
        Ok(item) => {
            // println!("item {:?}", item);

            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            item.encode(&mut consumer).unwrap();

            let encoded = consumer.as_ref();

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

pub fn encoding_relation_random<T>(data: &[u8])
where
    T: Encodable + Decodable + std::fmt::Debug + PartialEq + Eq,
{
    let mut producer = FromSlice::new(data);

    match T::decode_canonical(&mut producer) {
        Ok(item) => {
            // println!("item {:?}", item);

            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            item.encode(&mut consumer).unwrap();

            let mut producer_2 = FromSlice::new(consumer.as_ref());

            match T::decode_canonical(&mut producer_2) {
                Ok(decoded_again) => {
                    assert_eq!(item, decoded_again);
                }
                Err(err) => {
                    println!("{:?}", err);
                    panic!("Could not decode again, argh!")
                }
            }
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

pub fn relative_encoding_roundtrip<T, R, C>(
    subject: T,
    reference: R,
    consumer: &mut TestConsumer<u8, u16, ()>,
) where
    T: std::fmt::Debug + PartialEq + Eq + RelativeEncodable<R> + RelativeDecodable<R>,
    R: std::fmt::Debug,
    C: BulkConsumer<Item = u8>,
{
    //println!("item {:?}", subject);
    //println!("ref {:?}", reference);

    if let Err(_err) = subject.relative_encode(&reference, consumer) {
        return;
    }

    if let Err(_err) = consumer.flush() {
        return;
    }

    let mut new_vec = Vec::new();

    new_vec.extend_from_slice(consumer.consumed());

    // THis should eventually be a testproducer, when we are able to initialise one with some known data.
    let mut producer = FromBoxedSlice::from_vec(new_vec);

    // Check for correct errors
    let decoded_item = T::relative_decode_canonical(&reference, &mut producer).unwrap();

    assert_eq!(decoded_item, subject);
}

pub fn relative_encoding_random<R, T>(reference: R, data: &[u8])
where
    T: RelativeEncodable<R> + RelativeDecodable<R> + std::fmt::Debug,
    R: std::fmt::Debug,
{
    let mut producer = FromSlice::new(data);

    match T::relative_decode_canonical(&reference, &mut producer) {
        Ok(item) => {
            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            //  println!("item {:?}", item);
            //  println!("ref {:?}", reference);

            item.relative_encode(&reference, &mut consumer).unwrap();

            let encoded = consumer.as_ref();

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

pub fn relative_encoding_relation_random<R, T>(reference: R, data: &[u8])
where
    T: RelativeEncodable<R> + RelativeDecodable<R> + std::fmt::Debug + Eq,
    R: std::fmt::Debug,
{
    let mut producer = FromSlice::new(data);

    match T::relative_decode_relation(&reference, &mut producer) {
        Ok(item) => {
            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

            //  println!("item {:?}", item);
            //  println!("ref {:?}", reference);

            item.relative_encode(&reference, &mut consumer).unwrap();

            let mut producer_2 = FromSlice::new(consumer.as_ref());

            match T::relative_decode_relation(&reference, &mut producer_2) {
                Ok(decoded_again) => {
                    assert_eq!(item, decoded_again);
                }
                Err(err) => {
                    println!("{:?}", err);
                    panic!("Could not decode again, argh!")
                }
            }
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
