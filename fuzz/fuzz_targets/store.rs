#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_fuzz::store::StoreOp;

fuzz_target!(|data: Vec<
    StoreOp<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, bool>,
>| {
    // fuzzed code goes here
});
