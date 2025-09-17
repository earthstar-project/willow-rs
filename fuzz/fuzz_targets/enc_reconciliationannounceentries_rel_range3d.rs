#![no_main]

use wgps::messages::{ReconciliationAnnounceEntries, ReconciliationSendFingerprint};
use willow_data_model::grouping::Range3d;
use willow_fuzz::placeholder_params::{FakeFingerprint, FakeNamespaceId, FakeSubspaceId};

use ufotofu_codec::{fuzz_relative_known_size, Blame};

fuzz_relative_known_size!(
    ReconciliationAnnounceEntries<16, 16, 16, FakeSubspaceId>;
    Range3d<16, 16, 16, FakeSubspaceId>;
    Blame;
    |msg: &ReconciliationAnnounceEntries<16, 16, 16, FakeSubspaceId>, range:  &Range3d<16, 16, 16, FakeSubspaceId>| {
        true
    }

);
