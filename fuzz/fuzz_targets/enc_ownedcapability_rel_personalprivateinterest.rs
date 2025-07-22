#![no_main]

use meadowcap::{AccessMode, OwnedCapability, SillyPublicKey, SillySig};
use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::grouping::AreaSubspace;
use willow_pio::PersonalPrivateInterest;

fuzz_relative_basic!(
    OwnedCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>;
    PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>;
    Blame;
    |cap :  & OwnedCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,  interest:  &PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>| {

        if let AreaSubspace::Id(id) = cap.granted_area().subspace() {
            if interest.private_interest().subspace_id() != id {

                return false
            }
        }



    cap.access_mode() == AccessMode::Read &&
    interest.private_interest().subspace_id() == cap.progenitor() &&
    cap.granted_namespace() == interest.private_interest().namespace_id() &&
    cap.granted_area().path().is_prefix_of(interest.private_interest().path())
    && cap.receiver() == interest.user_key()
    }
);
