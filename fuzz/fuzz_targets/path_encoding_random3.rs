#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::Path;
use willow_fuzz::encode::encoding_random;

const MCL: usize = 4;
const MCC: usize = 4;
const MPL: usize = 16;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<Path<MCL, MCC, MPL>>(data).await;
    });
});
