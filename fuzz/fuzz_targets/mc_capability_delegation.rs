#![no_main]

use libfuzzer_sys::fuzz_target;
use meadowcap::{FailedDelegationError, McCapability, SillyPublicKey, SillySecret, SillySig};
use willow_data_model::grouping::Area;

fuzz_target!(|data: (
    SillySecret,
    SillyPublicKey,
    Area<16, 16, 16, SillyPublicKey>,
    McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>,
)| {
    let (secret, new_user, area, mc_cap) = data;

    let area_is_included = mc_cap.granted_area().includes_area(&area);
    let is_correct_secret = mc_cap.receiver() == &secret.corresponding_public_key();

    match mc_cap.delegate(&secret, &new_user, &area) {
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
                assert_eq!(&pub_key, mc_cap.receiver());
                assert!(secret.corresponding_public_key() != pub_key);
            }
        },
    }
});
