#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::{consumer::IntoVec, producer::FromVec};
use willow_data_model::{
    encoding::path::{decode_path, encode_path},
    path::PathRc,
};

const MCL: usize = 111111;
const MCC: usize = 111111;
const MPL: usize = 111111;

fuzz_target!(|path: PathRc<MCL, MCC, MPL>| {
    // Use a TestConsumer instead of IntoVec
    let mut consumer = IntoVec::<u8>::new();

    smol::block_on(async {
        // I want to test that the correct error is returned given the how BAD our testconsumer is
        encode_path::<MCL, MCC, _, _>(&path, &mut consumer)
            .await
            .unwrap();

        // THis should eventually be a testproducer
        let mut producer = FromVec::new(consumer.into_vec());

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
