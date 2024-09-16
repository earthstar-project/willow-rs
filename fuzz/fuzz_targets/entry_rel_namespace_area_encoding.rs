#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::grouping::Area;
use willow_data_model::Entry;
use willow_fuzz::encode::relative_encoding_canonical_roundtrip;
use willow_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Area<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry, area, mut consumer) = data;

    if !area.includes_entry(&entry) {
        return;
    }

    let namespace = entry.namespace_id().clone();

    relative_encoding_canonical_roundtrip::<
        Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
        (EsNamespaceId, Area<16, 16, 16, IdentityId>),
        TestConsumer<u8, u16, ()>,
    >(entry, (namespace, area), &mut consumer)
});
