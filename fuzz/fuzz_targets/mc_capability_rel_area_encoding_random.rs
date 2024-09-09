#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::McCapability;
use willow_data_model::grouping::Area;
use willow_fuzz::encode::relative_encoding_random;
use willow_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (&[u8], Area<3, 3, 3, SillyPublicKey>,)| {
    let (random_bytes, out) = data;

    relative_encoding_random::<
        Area<3, 3, 3, SillyPublicKey>,
        McCapability<3, 3, 3, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    >(out, random_bytes)
});
