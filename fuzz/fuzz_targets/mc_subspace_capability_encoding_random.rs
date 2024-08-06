#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::McSubspaceCapability;
use willow_fuzz::encode::encoding_random;
use willow_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<
            McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
        >(data)
        .await;
    });
});
