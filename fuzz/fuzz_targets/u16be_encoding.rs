#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_encoding::U16BE;
use willow_fuzz::encode::encoding_roundtrip;

fuzz_target!(|data: (u16, TestConsumer<u8, u16, ()>)| {
    let (n, mut consumer) = data;

    encoding_roundtrip::<_, TestConsumer<u8, u16, ()>>(U16BE::from(n), &mut consumer);
});
