#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::{consumer::TestConsumer, producer::FromVec, BufferedConsumer};
use willow_data_model::{
    encoding::parameters::{Decoder, Encoder},
    path::PathRc,
};

const MCL: usize = 111111;
const MCC: usize = 111111;
const MPL: usize = 111111;

fuzz_target!(|data: (PathRc<MCL, MCC, MPL>, TestConsumer<u8, u8, u8>)| {
    let (path, mut consumer) = data;

    smol::block_on(async {
        let consumer_should_error = consumer.should_error();

        if let Err(_err) = path.encode(&mut consumer).await {
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
        let decoded_path = PathRc::decode(&mut producer).await.unwrap();

        assert_eq!(decoded_path, path);
    });
});
