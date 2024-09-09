#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::Path;
use willow_fuzz::encode::encoding_random;

const MCL: usize = 2;
const MCC: usize = 3;
const MPL: usize = 3;

fuzz_target!(|data: &[u8]| {
    encoding_random::<Path<MCL, MCC, MPL>>(data);
});
