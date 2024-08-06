#![no_main]

use earthstar::identity_id::IdentityIdentifier as IdentityId;
use earthstar::namespace_id::NamespaceIdentifier as EsNamespaceId;

use willow_data_model::Entry;

use libfuzzer_sys::fuzz_target;
use willow_data_model_fuzz::{encode::encoding_random, placeholder_params::FakePayloadDigest};

fuzz_target!(|data: &[u8]| {
    smol::block_on(async {
        encoding_random::<Entry<3, 3, 3, EsNamespaceId, IdentityId, FakePayloadDigest>>(data).await;
    });
});
