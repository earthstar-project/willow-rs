#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::encoding::U64BE;
use willow_fuzz::encode::encoding_random;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<U64BE>(data).await;
    });
});
