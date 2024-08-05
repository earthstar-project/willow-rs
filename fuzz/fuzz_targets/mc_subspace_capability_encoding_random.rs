#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::subspace_capability::McSubspaceCapability;
use willow_data_model_fuzz::encode::encoding_random;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<
            McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
        >(data)
        .await;
    });
});
