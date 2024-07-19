use ufotofu::{
    local_nb::{
        consumer::TestConsumer,
        producer::{FromVec, SliceProducer},
        BufferedConsumer, BulkConsumer,
    },
    sync::consumer::IntoVec,
};
use willow_data_model::{
    encoding::{
        error::DecodeError,
        parameters::{Decodable, Encodable},
        relativity::{RelativeDecodable, RelativeEncodable},
    },
    path::*,
};

pub async fn encoding_roundtrip<T, C>(item: T, consumer: &mut TestConsumer<u8, u16, ()>)
where
    T: Encodable + Decodable + std::fmt::Debug + PartialEq + Eq,
    C: BulkConsumer<Item = u8>,
{
    let consumer_should_error = consumer.should_error();

    if let Err(_err) = item.encode(consumer).await {
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
    let decoded_item = T::decode(&mut producer).await.unwrap();

    assert_eq!(decoded_item, item);
}

pub async fn encoding_random<T>(data: &[u8])
where
    T: Encodable + Decodable,
{
    let mut producer = SliceProducer::new(data);

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
    let consumer_should_error = consumer.should_error();

    if let Err(_err) = subject.relative_encode(&reference, consumer).await {
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
    let decoded_item = T::relative_decode(&reference, &mut producer).await.unwrap();

    assert_eq!(decoded_item, subject);
}

pub async fn relative_encoding_random<R, T>(reference: R, data: &[u8])
where
    T: RelativeEncodable<R> + RelativeDecodable<R> + std::fmt::Debug,
    R: std::fmt::Debug,
{
    let mut producer = SliceProducer::new(data);

    match T::relative_decode(&reference, &mut producer).await {
        Ok(item) => {
            // It decoded to a valid item! Gasp!
            // Can we turn it back into the same encoding?
            let mut consumer = IntoVec::<u8>::new();

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

pub fn test_successor<const MCL: usize, const MCC: usize, const MPL: usize>(
    baseline: PathRc<MCL, MCC, MPL>,
    candidate: PathRc<MCL, MCC, MPL>,
    max_path: PathRc<MCL, MCC, MPL>,
) {
    let successor = baseline.successor();

    match successor {
        None => {
            if baseline != max_path {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo")
            }
        }
        Some(successor) => {
            if successor <= baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");

                panic!("successor was not greater than the path it was derived from! BooooOoooOOo")
            }

            if candidate < successor && candidate > baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");

                panic!("the successor generated was NOT the immediate successor! BooooOOOOo!")
            }
        }
    }
}

pub fn test_successor_of_prefix<const MCL: usize, const MCC: usize, const MPL: usize>(
    baseline: PathRc<MCL, MCC, MPL>,
    candidate: PathRc<MCL, MCC, MPL>,
    unsucceedable: &[PathRc<MCL, MCC, MPL>],
) {
    let prefix_successor = baseline.successor_of_prefix();

    match prefix_successor {
        None => {
            if !unsucceedable.iter().any(|unsuc| unsuc == &baseline) {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo\n\n\n\n");
            }
        }
        Some(prefix_successor) => {
            if prefix_successor <= baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("the successor is meant to be greater than the baseline, but wasn't!! BOOOOOOOOO\n\n\n\n");
            }

            if prefix_successor.is_prefixed_by(&baseline) {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("successor was prefixed by the path it was derived from! BoooOOooOOooOo\n\n\n\n");
            }

            if !baseline.is_prefix_of(&candidate)
                && candidate < prefix_successor
                && candidate > baseline
            {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);

                panic!(
                    "the successor generated was NOT the immediate prefix successor! BooooOOOOo!\n\n\n\n"
                );
            }
        }
    }
}
