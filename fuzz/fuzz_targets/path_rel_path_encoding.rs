#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::{encoding::relativity::RelativeEncoding, path::PathRc};
use willow_data_model_fuzz::relative_encoding_roundtrip;

const MCL: usize = 16;
const MCC: usize = 16;
const MPL: usize = 16;

fuzz_target!(|data: (
    PathRc<MCL, MCC, MPL>,
    PathRc<MCL, MCC, MPL>,
    TestConsumer<u8, u16, ()>
)| {
    let (path_sub, path_ref, mut consumer) = data;

    let relativity = RelativeEncoding {
        subject: path_sub,
        reference: path_ref,
    };

    smol::block_on(async {
        relative_encoding_roundtrip::<
            PathRc<MCL, MCC, MPL>,
            PathRc<MCL, MCC, MPL>,
            TestConsumer<u8, u16, ()>,
        >(relativity, &mut consumer)
        .await;
    });
});
