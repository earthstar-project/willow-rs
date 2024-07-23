#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::path::Path;
use willow_data_model_fuzz::encode::relative_encoding_random;

const MCL: usize = 2;
const MCC: usize = 3;
const MPL: usize = 3;

fuzz_target!(|data: (&[u8], Path<MCL, MCC, MPL>)| {
    let (random_bytes, ref_path) = data;

    smol::block_on(async {
        relative_encoding_random::<Path<MCL, MCC, MPL>, Path<MCL, MCC, MPL>>(
            ref_path,
            random_bytes,
        )
        .await;
    });
});
