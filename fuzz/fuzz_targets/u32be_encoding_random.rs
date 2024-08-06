#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::encoding::U32BE;
use willow_data_model_fuzz::encode::encoding_random;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<U32BE>(data).await;
    });
});
