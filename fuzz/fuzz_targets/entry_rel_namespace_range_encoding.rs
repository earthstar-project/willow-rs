#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::entry::Entry;
use willow_data_model::grouping::range_3d::Range3d;
use willow_data_model_fuzz::encode::relative_encoding_roundtrip;
use willow_data_model_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Range3d<16, 16, 16, IdentityId>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry, range_3d, mut consumer) = data;

    if !range_3d.includes_entry(&entry) {
        return;
    }

    let namespace = entry.namespace_id.clone();

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
            (EsNamespaceId, Range3d<16, 16, 16, IdentityId>),
            TestConsumer<u8, u16, ()>,
        >(entry, (namespace, range_3d), &mut consumer)
        .await;
    });
});
