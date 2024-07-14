#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::path::PathRc;
use willow_data_model_fuzz::relative_encoding_random;

const MCL: usize = 16;
const MCC: usize = 16;
const MPL: usize = 16;

fuzz_target!(|data: (&[u8], PathRc<MCL, MCC, MPL>)| {
    let (random_bytes, ref_path) = data;

    smol::block_on(async {
        relative_encoding_random::<PathRc<MCL, MCC, MPL>, PathRc<MCL, MCC, MPL>>(
            ref_path,
            random_bytes,
        )
        .await;
    });
});