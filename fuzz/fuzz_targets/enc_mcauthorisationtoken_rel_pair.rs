#![no_main]

use meadowcap::{
    AccessMode, CommunalCapability, McAuthorisationToken, PersonalPrivateInterest, SillyPublicKey,
    SillySig,
};
use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::{AuthorisationToken, AuthorisedEntry, Entry};
use willow_fuzz::placeholder_params::FakePayloadDigest;

fuzz_relative_basic!(
    McAuthorisationToken<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>;
    (           AuthorisedEntry<16, 16, 16, SillyPublicKey, SillyPublicKey, FakePayloadDigest, McAuthorisationToken<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>>,
        Entry<16, 16, 16, SillyPublicKey, SillyPublicKey, FakePayloadDigest>,);
    Blame;
    |token :  &McAuthorisationToken<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,  pair:  &(           AuthorisedEntry<16, 16, 16, SillyPublicKey, SillyPublicKey, FakePayloadDigest, McAuthorisationToken<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>>,
    Entry<16, 16, 16, SillyPublicKey, SillyPublicKey, FakePayloadDigest>,)| {
token.capability.granted_namespace() == pair.1.namespace_id() && token.capability.granted_area().includes_entry(&pair.1) &&
   token.is_authorised_write(&pair.1)
    }
);
