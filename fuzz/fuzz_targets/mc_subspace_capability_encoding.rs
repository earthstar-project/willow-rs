#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::McSubspaceCapability;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_fuzz::encode::encoding_canonical_roundtrip;
use willow_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (
    McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Vec<SillyPublicKey>,
    TestConsumer<u8, u16, ()>
)| {
    let (mc_cap, delegees, mut consumer) = data;

    let mut cap_with_delegees = mc_cap.clone();
    let mut last_receiver = mc_cap.receiver().clone();

    for delegee in delegees {
        cap_with_delegees = cap_with_delegees
            .delegate(&last_receiver.corresponding_secret_key(), &delegee)
            .unwrap();
        last_receiver = delegee;
    }

    encoding_canonical_roundtrip::<
        McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
        TestConsumer<u8, u16, ()>,
    >(cap_with_delegees, &mut consumer)
});
