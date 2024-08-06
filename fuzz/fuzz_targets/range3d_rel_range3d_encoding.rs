#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::grouping::Range3d;
use willow_fuzz::encode::relative_encoding_roundtrip;

fuzz_target!(|data: (
    Range3d<16, 16, 16, IdentityId>,
    Range3d<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (ran, reference, mut consumer) = data;

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Range3d<16, 16, 16, IdentityId>,
            Range3d<16, 16, 16, IdentityId>,
            TestConsumer<u8, u16, ()>,
        >(ran, reference, &mut consumer)
        .await;
    });
});
