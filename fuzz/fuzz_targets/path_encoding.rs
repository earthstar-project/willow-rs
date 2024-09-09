#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::Path;
use willow_fuzz::encode::encoding_roundtrip;

const MCL: usize = 300;
const MCC: usize = 300;
const MPL: usize = 300;

fuzz_target!(|data: (Path<MCL, MCC, MPL>, TestConsumer<u8, u16, ()>)| {
    let (path, mut consumer) = data;

    encoding_roundtrip::<_, TestConsumer<u8, u16, ()>>(path, &mut consumer);
});
