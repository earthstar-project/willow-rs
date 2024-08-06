#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use libfuzzer_sys::fuzz_target;
use willow_data_model::grouping::Area;
use willow_data_model_fuzz::encode::relative_encoding_random_less_strict;

fuzz_target!(|data: (&[u8], Area<16, 16, 16, IdentityId>)| {
    // fuzzed code goes here
    let (random_bytes, area) = data;

    smol::block_on(async {
        relative_encoding_random_less_strict::<
            Area<16, 16, 16, IdentityId>,
            Area<16, 16, 16, IdentityId>,
        >(area, random_bytes)
        .await;
    });
});
