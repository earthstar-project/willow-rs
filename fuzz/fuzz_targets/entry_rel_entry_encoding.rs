#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::Entry;
use willow_fuzz::encode::relative_encoding_canonical_roundtrip;
use willow_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry_sub, entry_ref, mut consumer) = data;

    relative_encoding_canonical_roundtrip::<
        Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
        Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
        TestConsumer<u8, u16, ()>,
    >(entry_sub, entry_ref, &mut consumer);
});
