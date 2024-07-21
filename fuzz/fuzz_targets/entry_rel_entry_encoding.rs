#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;
use libfuzzer_sys::fuzz_target;
use ufotofu::local_nb::consumer::TestConsumer;
use willow_data_model::entry::Entry;
use willow_data_model_fuzz::encode::relative_encoding_roundtrip;
use willow_data_model_fuzz::placeholder_params::FakePayloadDigest;

fuzz_target!(|data: (
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
    TestConsumer<u8, u16, ()>
)| {
    let (entry_sub, entry_ref, mut consumer) = data;

    smol::block_on(async {
        relative_encoding_roundtrip::<
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
            Entry<16, 16, 16, EsNamespaceId, IdentityId, FakePayloadDigest>,
            TestConsumer<u8, u16, ()>,
        >(entry_sub, entry_ref, &mut consumer)
        .await;
    });
});
