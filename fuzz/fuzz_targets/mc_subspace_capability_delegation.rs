#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::subspace_capability::McSubspaceCapability;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySecret, SillySig};

fuzz_target!(|data: (
    SillySecret,
    SillyPublicKey,
    McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Vec<SillyPublicKey>
)| {
    let (secret, new_user, mc_subspace_cap, delegees) = data;

    let mut cap_with_delegees = mc_subspace_cap.clone();
    let mut last_receiver = mc_subspace_cap.receiver().clone();

    for delegee in delegees {
        cap_with_delegees = cap_with_delegees
            .delegate(&last_receiver.corresponding_secret_key(), &delegee)
            .unwrap();
        last_receiver = delegee;
    }

    let is_correct_secret = cap_with_delegees.receiver() == &secret.corresponding_public_key();

    match cap_with_delegees.delegate(&secret, &new_user) {
        Ok(delegated_cap) => {
            assert!(is_correct_secret);

            assert_eq!(delegated_cap.receiver(), &new_user);
        }
        Err(_) => {
            assert_eq!(&last_receiver, cap_with_delegees.receiver());
            assert!(secret.corresponding_public_key() != last_receiver);
        }
    }
});
