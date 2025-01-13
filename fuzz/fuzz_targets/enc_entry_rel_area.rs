#![no_main]

use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::{grouping::Area, Entry};
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakePayloadDigest, FakeSubspaceId};

fuzz_relative_all!(Entry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; (FakeNamespaceId, Area<16, 16, 16, FakeSubspaceId>); Blame; Blame; |entry: &Entry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>, (namespace, area): &(FakeNamespaceId, Area<16, 16, 16, FakeSubspaceId>)| {
  entry.namespace_id() == namespace && area.includes_entry(entry)
});
