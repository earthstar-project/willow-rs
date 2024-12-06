#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Entry;

fuzz_absolute_all!(Entry<3, 3, 3, EsNamespaceId, IdentityId, FakePayloadDigest>);
