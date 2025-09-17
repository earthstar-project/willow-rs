#![no_main]

use wgps::messages::{ReconciliationAnnounceEntries, ReconciliationSendEntry};
use willow_data_model::{grouping::Range3d, AuthorisedEntry};
use willow_fuzz::placeholder_params::{
    FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
};

use ufotofu_codec::{fuzz_relative_known_size, Blame};

fuzz_relative_known_size!(
    ReconciliationSendEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>;
    (
        (&FakeNamespaceId, &Range3d<16, 16, 16, FakeSubspaceId>),
        &AuthorisedEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>,
    );
    Blame;
    |msg: &ReconciliationSendEntry<16, 16, 16, FakeSubspaceId>, range:  &(
        (&FakeNamespaceId, &Range3d<16, 16, 16, FakeSubspaceId>),
        &AuthorisedEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>,
    )| {
        true
    }

);
