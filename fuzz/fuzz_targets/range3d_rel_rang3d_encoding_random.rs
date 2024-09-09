#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use libfuzzer_sys::fuzz_target;
use willow_data_model::grouping::Range3d;
use willow_fuzz::encode::relative_encoding_relation_random;

fuzz_target!(|data: (&[u8], Range3d<16, 16, 16, IdentityId>)| {
    // fuzzed code goes here
    let (random_bytes, area) = data;

    relative_encoding_relation_random::<
        Range3d<16, 16, 16, IdentityId>,
        Range3d<16, 16, 16, IdentityId>,
    >(area, random_bytes);
});
