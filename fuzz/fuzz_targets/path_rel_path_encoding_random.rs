#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::Path;
use willow_fuzz::encode::relative_encoding_canonical_random;

const MCL: usize = 300;
const MCC: usize = 300;
const MPL: usize = 300;

fuzz_target!(|data: (&[u8], Path<MCL, MCC, MPL>)| {
    let (random_bytes, ref_path) = data;

    relative_encoding_canonical_random::<Path<MCL, MCC, MPL>, Path<MCL, MCC, MPL>>(
        &ref_path,
        random_bytes,
    );
});
