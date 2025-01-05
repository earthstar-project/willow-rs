#![no_main]

use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::{grouping::Range3d, Entry};
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakePayloadDigest, FakeSubspaceId};

fuzz_relative_all!(Entry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; (FakeNamespaceId, Range3d<16, 16, 16, FakeSubspaceId>); Blame; Blame; |entry: &Entry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>, (namespace, range): &(FakeNamespaceId, Range3d<16, 16, 16, FakeSubspaceId>)| {
  entry.namespace_id() == namespace && range.includes_entry(entry)
});
