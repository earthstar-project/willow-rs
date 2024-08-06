#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::grouping::Area;
use willow_fuzz::encode::relative_encoding_roundtrip;

fuzz_target!(|data: (
    Area<16, 16, 16, IdentityId>,
    Area<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (a, out, mut consumer) = data;

    if !out.includes_area(&a) {
        return;
    }

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Area<16, 16, 16, IdentityId>,
            Area<16, 16, 16, IdentityId>,
            TestConsumer<u8, u16, ()>,
        >(a, out, &mut consumer)
        .await;
    });
});
