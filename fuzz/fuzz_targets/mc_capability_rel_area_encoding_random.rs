#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::mc_capability::McCapability;
use willow_data_model::grouping::area::Area;
use willow_data_model_fuzz::encode::relative_encoding_random_less_strict;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (&[u8], Area<3, 3, 3, SillyPublicKey>,)| {
    let (random_bytes, out) = data;

    smol::block_on(async {
        relative_encoding_random_less_strict::<
            Area<3, 3, 3, SillyPublicKey>,
            McCapability<3, 3, 3, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
        >(out, random_bytes)
        .await;
    });
});
