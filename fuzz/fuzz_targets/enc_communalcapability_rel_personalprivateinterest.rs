#![no_main]

use meadowcap::{
    AccessMode, CommunalCapability, PersonalPrivateInterest, SillyPublicKey, SillySig,
};
use ufotofu_codec::{fuzz_relative_basic, Blame};

fuzz_relative_basic!(
    CommunalCapability<16, 16, 16, SillyPublicKey, SillyPublicKey, SillySig>;
    PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>;
    Blame;
    |cap :  &CommunalCapability<16, 16, 16, SillyPublicKey, SillyPublicKey, SillySig>,  interest:  &PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>| {
    cap.access_mode() == AccessMode::Read &&
    interest.private_interest().subspace_id() == cap.progenitor() &&
    cap.granted_namespace() == interest.private_interest().namespace_id() &&
    cap.granted_area().path().is_prefix_of(interest.private_interest().path())
    && cap.receiver() == interest.user_key()
    }
);
