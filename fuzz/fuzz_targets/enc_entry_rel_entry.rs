#![no_main]

use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::Entry;
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakePayloadDigest, FakeSubspaceId};

fuzz_relative_basic!(Entry<3, 3, 3, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; Entry<3, 3, 3, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>; Blame; |_e1, _e2| true);
