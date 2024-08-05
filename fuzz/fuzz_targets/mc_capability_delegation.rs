#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{mc_capability::McCapability, FailedDelegationError};
use willow_data_model::grouping::area::Area;
use willow_data_model_fuzz::silly_sigs::{SillyPublicKey, SillySecret, SillySig};

fuzz_target!(|data: (
    SillySecret,
    SillyPublicKey,
    Area<16, 16, 16, SillyPublicKey>,
    McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
    Vec<SillyPublicKey>
)| {
    let (secret, new_user, area, mc_cap, delegees) = data;

    let mut cap_with_delegees = mc_cap.clone();
    let mut last_receiver = mc_cap.receiver().clone();
    let granted_area = mc_cap.granted_area();

    for delegee in delegees {
        cap_with_delegees = cap_with_delegees
            .delegate(
                &last_receiver.corresponding_secret_key(),
                &delegee,
                &granted_area,
            )
            .unwrap();
        last_receiver = delegee;
    }

    let area_is_included = cap_with_delegees.granted_area().includes_area(&area);
    let is_correct_secret = cap_with_delegees.receiver() == &secret.corresponding_public_key();

    match cap_with_delegees.delegate(&secret, &new_user, &area) {
        Ok(delegated_cap) => {
            assert!(area_is_included);
            assert!(is_correct_secret);

            assert_eq!(delegated_cap.granted_area(), area);
            assert_eq!(delegated_cap.receiver(), &new_user);
        }
        Err(err) => match err {
            FailedDelegationError::AreaNotIncluded {
                excluded_area,
                claimed_receiver: _,
            } => {
                assert!(!area_is_included);
                assert_eq!(excluded_area, area);
            }
            FailedDelegationError::WrongSecretForUser(pub_key) => {
                assert_eq!(&pub_key, cap_with_delegees.receiver());
                assert!(secret.corresponding_public_key() != pub_key)
            }
        },
    }
});
