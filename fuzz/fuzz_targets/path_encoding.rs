#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::{consumer::TestConsumer, producer::FromVec, BufferedConsumer};
use willow_data_model::{
    encoding::path::{decode_path, encode_path},
    path::PathRc,
};

const MCL: usize = 111111;
const MCC: usize = 111111;
const MPL: usize = 111111;

fuzz_target!(|data: (PathRc<MCL, MCC, MPL>, TestConsumer<u8, u8, u8>)| {
    let (path, mut consumer) = data;

    smol::block_on(async {
        let consumer_should_error = consumer.should_error();

        if let Err(_err) = encode_path::<MCL, MCC, _, _>(&path, &mut consumer).await {
            assert!(consumer_should_error);
            return;
        }

        consumer.flush().await.unwrap();

        let mut new_vec = Vec::new();

        new_vec.extend_from_slice(consumer.as_ref());

        // THis should eventually be a testproducer, when we can initialise one of those with some known data.
        let mut producer = FromVec::new(new_vec);

        // Check for correct errors
        let decoded_path = decode_path::<MCL, MCC, _, PathRc<MCL, MCC, MPL>>(&mut producer)
            .await
            .unwrap();

        assert_eq!(decoded_path, path);
    });
});

// ALSO: fuzzer generates random bytes and you see how they are handled
// If they decode, you encode again, and check for equality again (remember to check against what you actually decoded)
// create fromsliceproducer from random bytes - fromsliceproduces exposes offset so you know how much was decoded.
