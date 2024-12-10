#![no_main]

use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::Entry;
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakePayloadDigest, FakeSubspaceId};

fuzz_relative_all!(Entry<3, 3, 3, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; Entry<3, 3, 3, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; Blame; Blame);
