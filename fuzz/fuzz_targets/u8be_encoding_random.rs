#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_encoding::U8BE;
use willow_fuzz::encode::encoding_random;

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<U8BE>(data).await;
    });
});
