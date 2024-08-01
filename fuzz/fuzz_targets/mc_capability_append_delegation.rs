#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{mc_capability::McCapability, Delegation, InvalidDelegationError};
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_target!(|data: (
    Delegation<16, 16, 16, SillyPublicKey, SillySig>,
    McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Vec<SillyPublicKey>
)| {
    let (delegation, mc_cap, delegees) = data;

    let mut mut_cap = mc_cap.clone();

    let mut last_receiver = mut_cap.receiver().clone();
    let granted_area = mut_cap.granted_area();

    for delegee in delegees {
        mut_cap = mut_cap
            .delegate(
                &last_receiver.corresponding_secret_key(),
                &delegee,
                &granted_area,
            )
            .unwrap();
        last_receiver = delegee;
    }

    let claimed_area = delegation.area().clone();
    let delegation_user = delegation.user().clone();
    let delegation_sig = delegation.signature().clone();

    let granted_area_includes_delegation = mut_cap.granted_area().includes_area(delegation.area());

    let actual_receiver_secret = mc_cap.receiver().corresponding_secret_key();

    match mut_cap.append_existing_delegation(delegation) {
        Ok(_) => {
            println!("yay");
            assert!(granted_area_includes_delegation);

            // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
            let expected_cap =
                mc_cap.delegate(&actual_receiver_secret, &delegation_user, &claimed_area);

            match expected_cap {
                Ok(cap) => {
                    assert_eq!(cap, mut_cap);
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
                assert_eq!(&expected_signatory, mut_cap.receiver());
                assert_eq!(delegation_user, claimed_receiver);

                // Because there is only one user who can delegate a given capability, we know what it should look like given the same new_area and new_user.
                let expected_cap =
                    mc_cap.delegate(&actual_receiver_secret, &delegation_user, &claimed_area);

                match expected_cap {
                    Ok(cap) => match cap.delegations().last() {
                        Some(valid_delegation) => {
                            assert!(valid_delegation.signature() != &delegation_sig)
                        }
                        None => {
                            unreachable!(
                                "We just made a delegation, this really should not happen!"
                            )
                        }
                    },
                    Err(_) => panic!("The expected cap should have been fine..."),
                }
            }
        },
    }
});
