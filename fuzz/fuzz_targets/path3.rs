#![no_main]

use libfuzzer_sys::fuzz_target;
// use willow_data_model::path::*;
use willow_data_model_fuzz::path::*;

fuzz_target!(|data: (CreatePath, CreatePath)| {
    let (cp1, cp2) = data;

    let ctrl1 = create_path_rc::<4, 4, 16>(&cp1);
    let ctrl2 = create_path_rc::<4, 4, 16>(&cp2);

    let p1 = create_path::<4, 4, 16>(&cp1);
    let p2 = create_path::<4, 4, 16>(&cp2);

    match (ctrl1, p1) {
        (Err(_), Err(_)) => { /* no-op */ }
        (Ok(ctrl1), Ok(p1)) => {
            match (ctrl2, p2) {
                (Err(_), Err(_)) => { /* no-op */ }
                (Ok(ctrl2), Ok(p2)) => {
                    assert_isomorphic_paths(&ctrl1, &ctrl2, &p1, &p2);
                }
                _ => {
                    panic!(
                        "Create_path_rc and create_path must either both succeeed or both fail."
                    );
                }
            }
        }
        _ => {
            panic!("Create_path_rc and create_path must either both succeeed or both fail.");
        }
    }
});
