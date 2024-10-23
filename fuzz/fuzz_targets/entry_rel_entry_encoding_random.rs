#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use willow_data_model::Entry;
use willow_fuzz::encode::relative_encoding_random;
use willow_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    &[u8],
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
)| {
    let (random_bytes, ref_entry) = data;

    relative_encoding_random::<
        Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
        Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    >(&ref_entry, random_bytes)
});
