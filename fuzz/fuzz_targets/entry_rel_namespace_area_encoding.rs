#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::entry::Entry;
use willow_data_model::grouping::area::Area;
use willow_data_model_fuzz::encode::relative_encoding_roundtrip;
use willow_data_model_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Area<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry, area, mut consumer) = data;

    if !area.includes_entry(&entry) {
        return;
    }

    let namespace = entry.get_namespace_id().clone();

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
            (EsNamespaceId, Area<16, 16, 16, IdentityId>),
            TestConsumer<u8, u16, ()>,
        >(entry, (namespace, area), &mut consumer)
        .await;
    });
});
