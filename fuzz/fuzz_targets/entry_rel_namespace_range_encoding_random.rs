#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use willow_data_model::grouping::Range3d;
use willow_data_model::Entry;
use willow_fuzz::encode::relative_encoding_relation_random;
use willow_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(
    |data: (&[u8], (EsNamespaceId, Range3d<16, 16, 16, IdentityId>))| {
        // fuzzed code goes here
        let (random_bytes, namespaced_range_3d) = data;

        relative_encoding_relation_random::<
            (EsNamespaceId, Range3d<16, 16, 16, IdentityId>),
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
        >(namespaced_range_3d, random_bytes);
    }
);
