#![no_main]

use wgps::messages::ReconciliationSendFingerprint;
use willow_data_model::grouping::Range3d;
use willow_fuzz::placeholder_params::{FakeFingerprint, FakeNamespaceId, FakeSubspaceId};

use ufotofu_codec::{fuzz_relative_known_size, Blame};

fuzz_relative_known_size!(
    ReconciliationSendFingerprint<16, 16, 16, FakeSubspaceId, FakeFingerprint>;
    Range3d<16, 16, 16, FakeSubspaceId>;
    Blame;
    |msg: &ReconciliationSendFingerprint<16, 16, 16, FakeSubspaceId, FakeFingerprint>, range:  &Range3d<16, 16, 16, FakeSubspaceId>| {
        true
    }

);
