#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_encoding::U64BE;
use willow_fuzz::encode::encoding_canonical_roundtrip;

fuzz_target!(|data: (u64, TestConsumer<u8, u16, ()>)| {
    let (n, mut consumer) = data;

    encoding_canonical_roundtrip::<_, TestConsumer<u8, u16, ()>>(U64BE::from(n), &mut consumer);
});
