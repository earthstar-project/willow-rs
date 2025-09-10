#![no_main]

use libfuzzer_sys::fuzz_target;
use ufotofu_codec::fuzz_relative_known_size;

fuzz_relative_known_size!(
    ReconciliationSendFingerprint<16, 16, 16, FakeSubspaceId, FakeFingerprint>;
    Range3d<16, 16, 16, FakeSubspaceId>;
    Blame;
    |msg: ReconciliationSendFingerprint<16, 16, 16, FakeSubspaceId, FakeFingerprint>, range:  Range3d<16, 16, 16, FakeSubspaceId>| {
        true
    }

);
