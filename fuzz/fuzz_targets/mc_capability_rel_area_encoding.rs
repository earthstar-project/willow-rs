#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::mc_capability::McCapability;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::grouping::area::Area;
use willow_data_model_fuzz::encode::relative_encoding_roundtrip;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (
    McCapability<3, 3, 3, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Area<3, 3, 3, SillyPublicKey>,
    Vec<SillyPublicKey>,
    TestConsumer<u8, u16, ()>
)| {
    let (mc_cap, out, delegees, mut consumer) = data;

    if !out.includes_area(&mc_cap.granted_area()) {
        return;
    }

    let mut cap_with_delegees = mc_cap.clone();
    let mut last_receiver = mc_cap.receiver().clone();
    let granted_area = mc_cap.granted_area();

    for delegee in delegees {
        cap_with_delegees = cap_with_delegees
            .delegate(
                &last_receiver.corresponding_secret_key(),
                &delegee,
                &granted_area,
            )
            .unwrap();
        last_receiver = delegee;
    }

    if !out.includes_area(&cap_with_delegees.granted_area()) {
        return;
    }

    smol::block_on(async {
        relative_encoding_roundtrip::<
            McCapability<3, 3, 3, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
            Area<3, 3, 3, SillyPublicKey>,
            TestConsumer<u8, u16, ()>,
        >(cap_with_delegees, out, &mut consumer)
        .await;
    });
});
