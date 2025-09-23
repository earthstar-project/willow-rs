#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{EnumerationDelegation, McEnumerationCapability, SillyPublicKey, SillySig};

fuzz_target!(|data: (
    EnumerationDelegation<SillyPublicKey, SillySig>,
    McEnumerationCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>
)| {
    let (delegation, mc_cap) = data;

    let mut mut_cap = mc_cap.clone();

    let delegation_user = delegation.user().clone();
    let delegation_sig = delegation.signature().clone();

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
                    assert!(valid_delegation.signature() != &delegation_sig);
                }
                Err(_) => panic!("The expected cap should have been fine..."),
            }
        }
    }
});
