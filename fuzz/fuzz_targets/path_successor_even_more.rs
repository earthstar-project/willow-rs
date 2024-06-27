#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::path::*;
use willow_data_model_fuzz::test_successor;

// MCL, MCC, MPL
fuzz_target!(|data: (PathRc<4, 4, 16>, PathRc<4, 4, 16>)| {
    let (baseline, candidate) = data;
    let max_path = PathRc::new(&[
        PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
    ])
    .unwrap();

    test_successor(baseline, candidate, max_path);
});
