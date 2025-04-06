#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{Delegation, InvalidDelegationError, McCapability, SillyPublicKey, SillySig};

fuzz_target!(|data: (
    Delegation<16, 16, 16, SillyPublicKey, SillySig>,
    McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
)| {
    let (delegation, mut mc_cap) = data;

    let claimed_area = delegation.area().clone();
    let delegation_user = delegation.user().clone();
    let delegation_sig = delegation.signature().clone();

    let granted_area_includes_delegation = mc_cap.granted_area().includes_area(delegation.area());

    let actual_receiver_secret = mc_cap.receiver().corresponding_secret_key();

    let cap_before_delegation = mc_cap.clone();

    match mc_cap.append_existing_delegation(delegation) {
        Ok(_) => {
            assert!(granted_area_includes_delegation);

            // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
            let expected_cap = cap_before_delegation.delegate(
                &actual_receiver_secret,
                &delegation_user,
                &claimed_area,
            );

            match expected_cap {
                Ok(cap) => {
                    assert_eq!(cap, mc_cap);
                }
                Err(_) => {
                    panic!("The delegation should not have been possible")
                }
            }
        }
        Err(err) => match err {
            InvalidDelegationError::AreaNotIncluded {
                excluded_area,
                claimed_receiver: _,
            } => {
                assert!(!granted_area_includes_delegation);
                assert_eq!(excluded_area, claimed_area);
            }
            InvalidDelegationError::InvalidSignature {
                expected_signatory,
                claimed_receiver,
                signature: _,
            } => {
                assert_eq!(&expected_signatory, mc_cap.receiver());
                assert_eq!(delegation_user, claimed_receiver);

                // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
                let expected_cap =
                    mc_cap.delegate(&actual_receiver_secret, &delegation_user, &claimed_area);

                match expected_cap {
                    Ok(cap) => {
                        let valid_delegation = cap.delegations().last().unwrap();
                        assert!(valid_delegation.signature() != &delegation_sig);
                    }
                    Err(_) => panic!("The expected cap should have been fine..."),
                }
            }
        },
    }
});
