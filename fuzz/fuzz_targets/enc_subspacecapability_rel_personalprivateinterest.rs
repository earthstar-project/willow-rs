#![no_main]

use meadowcap::{McSubspaceCapability, PersonalPrivateInterest, SillyPublicKey, SillySig};
use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::grouping::AreaSubspace;

fuzz_relative_basic!(
    McSubspaceCapability< SillyPublicKey, SillySig, SillyPublicKey, SillySig>;
    PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>;
    Blame;
    |cap :  &McSubspaceCapability< SillyPublicKey, SillySig, SillyPublicKey, SillySig>,  interest:  &PersonalPrivateInterest<16, 16, 16, SillyPublicKey, SillyPublicKey>| {

      *interest.private_interest().subspace_id() == AreaSubspace::Any &&
      interest.private_interest().namespace_id() == cap.granted_namespace()}
);
