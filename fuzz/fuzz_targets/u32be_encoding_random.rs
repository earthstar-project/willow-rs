#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_encoding::U32BE;
use willow_fuzz::encode::encoding_random;

fuzz_target!(|data: &[u8]| {
    encoding_random::<U32BE>(data);
});
