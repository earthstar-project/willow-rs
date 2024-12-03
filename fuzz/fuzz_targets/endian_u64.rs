#![no_main]

use libfuzzer_sys::fuzz_target;

use core::convert::Infallible;
use core::num::NonZeroUsize;

use ufotofu::consumer::TestConsumer;

use ufotofu_codec_endian::U64BE;

fuzz_target!(|data: (
    U64BE,
    U64BE,
    TestConsumer<u8, (), ()>,
    TestConsumer<u8, (), ()>,
    Vec<u8>,
    Box<[NonZeroUsize]>,
    Box<[NonZeroUsize]>,
    Box<[bool]>,
    Box<[bool]>,
)| {
    let (t1,
        t2,
        c1,
        c2,
        potential_encoding,
        exposed_items_sizes1,
        exposed_items_sizes2,
        yield_pattern1,
        yield_pattern2,) = data;

    panic!("rrrrrrrrr");

    // encoding_canonical_roundtrip::<_, TestConsumer<u8, u16, ()>>(entry, &mut consumer);
});