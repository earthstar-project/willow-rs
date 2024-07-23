#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::path::Path;
use willow_data_model_fuzz::encode::encoding_roundtrip;

const MCL: usize = 4;
const MCC: usize = 4;
const MPL: usize = 16;

fuzz_target!(|data: (Path<MCL, MCC, MPL>, TestConsumer<u8, u16, ()>)| {
    let (path, mut consumer) = data;

    smol::block_on(async {
        encoding_roundtrip::<_, TestConsumer<u8, u16, ()>>(path, &mut consumer).await;
    });
});