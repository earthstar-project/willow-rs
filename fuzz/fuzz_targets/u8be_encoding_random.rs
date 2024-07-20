#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::encoding::unsigned_int::U8BE;
use willow_data_model_fuzz::encoding_random;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<U8BE>(data).await;
    });
});
