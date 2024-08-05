#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::subspace_capability::McSubspaceCapability;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (
    (SillyPublicKey, SillySig),
    McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Vec<SillyPublicKey>
)| {
    let (delegation, mc_cap, delegees) = data;

    let mut mut_cap = mc_cap.clone();

    let mut last_receiver = mut_cap.receiver().clone();

    for delegee in delegees {
        mut_cap = mut_cap
            .delegate(&last_receiver.corresponding_secret_key(), &delegee)
            .unwrap();
        last_receiver = delegee;
    }

    let (delegation_user, delegation_sig) = delegation.clone();

    let actual_receiver_secret = mut_cap.receiver().corresponding_secret_key();

    let cap_before_delegation = mut_cap.clone();

    match mut_cap.append_existing_delegation(delegation) {
        Ok(_) => {
            // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
            let expected_cap =
                cap_before_delegation.delegate(&actual_receiver_secret, &delegation_user);

            match expected_cap {
                Ok(cap) => {
                    assert_eq!(cap, mut_cap);
                }
                Err(_) => {
                    panic!("The delegation should not have been possible")
                }
            }
        }
        Err(_) => {
            assert_eq!(cap_before_delegation.receiver(), mut_cap.receiver());

            // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
            let expected_cap = mut_cap.delegate(&actual_receiver_secret, &delegation_user);

            match expected_cap {
                Ok(cap) => {
                    let valid_delegation = cap.delegations().last().unwrap();
                    assert!(valid_delegation.1 != delegation_sig);
                }
                Err(_) => panic!("The expected cap should have been fine..."),
            }
        }
    }
});
