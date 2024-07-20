#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;

use willow_data_model::path::Path;
use willow_data_model_fuzz::relative_encoding_roundtrip;

const MCL: usize = 16;
const MCC: usize = 16;
const MPL: usize = 16;

fuzz_target!(|data: (
    Path<MCL, MCC, MPL>,
    Path<MCL, MCC, MPL>,
    TestConsumer<u8, u16, ()>
)| {
    let (path_sub, path_ref, mut consumer) = data;

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Path<MCL, MCC, MPL>,
            Path<MCL, MCC, MPL>,
            TestConsumer<u8, u16, ()>,
        >(path_sub, path_ref, &mut consumer)
        .await;
    });
});
