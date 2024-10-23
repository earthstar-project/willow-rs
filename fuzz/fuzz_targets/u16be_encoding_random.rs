#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_encoding::U16BE;
use willow_fuzz::encode::encoding_canonical_random;

fuzz_target!(|data: &[u8]| {
    encoding_canonical_random::<U16BE>(data);
});
