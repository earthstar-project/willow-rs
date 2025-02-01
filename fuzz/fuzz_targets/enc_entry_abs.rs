#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Entry;
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakePayloadDigest, FakeSubspaceId};

fuzz_absolute_all!(Entry<3, 3, 3, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest>);
