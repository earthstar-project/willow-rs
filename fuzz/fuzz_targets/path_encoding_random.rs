#![no_main]

use ufotofu::{common::consumer::IntoVec, local_nb::producer::SliceProducer};

use libfuzzer_sys::fuzz_target;
use willow_data_model::{
    encoding::{
        error::DecodeError,
        parameters::{Decoder, Encoder},
    },
    path::PathRc,
};

const MCL: usize = 111111;
const MCC: usize = 111111;
const MPL: usize = 111111;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        let mut producer = SliceProducer::new(data);

        match PathRc::<MCL, MCC, MPL>::decode(&mut producer).await {
            Ok(path) => {
                // It decoded to a valid path! Gasp!
                // Can we turn it back into the same encoding?
                let mut consumer = IntoVec::<u8>::new();

                path.encode(&mut consumer).await.unwrap();

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
