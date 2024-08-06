#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{AccessMode, McAuthorisationToken};
use signature::Signer;
use ufotofu::sync::consumer::IntoVec;
use willow_data_model::encoding::parameters_sync::Encodable;
use willow_data_model::entry::Entry;
use willow_data_model::parameters::IsAuthorisedWrite;
use willow_data_model_fuzz::{
    placeholder_params::FakePayloadDigest,
    silly_sigs::{SillyPublicKey, SillySig},
};

fuzz_target!(|data: (
    Entry<16, 16, 16, SillyPublicKey, SillyPublicKey, FakePayloadDigest>,
    McAuthorisationToken<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>
)| {
    let (entry, token) = data;

    let is_within_granted_area = token.capability.granted_area().includes_entry(&entry);
    let is_write_cap = token.capability.access_mode() == &AccessMode::Write;

    let mut consumer = IntoVec::<u8>::new();
    entry.encode(&mut consumer).unwrap();
    let message = consumer.into_vec();

    let expected_sig = token
        .capability
        .receiver()
        .corresponding_secret_key()
        .sign(&message);

    if token.is_authorised_write(&entry) {
        assert!(is_write_cap);
        assert!(is_within_granted_area);
        assert_eq!(token.signature, expected_sig);
    } else {
        assert!(!is_write_cap || !is_within_granted_area || token.signature != expected_sig);
    }
});
