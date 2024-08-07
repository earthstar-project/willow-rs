#![no_main]

use libfuzzer_sys::fuzz_target;
// use willow_data_model::path::*;
use willow_fuzz::path::*;

// MCL, MCC, MPL
fuzz_target!(|data: (PathRc<2, 3, 3>, PathRc<2, 3, 3>)| {
    let (baseline, candidate) = data;
    let max_path = PathRc::new(&[
        PathComponentBox::new(&[255, 255]).unwrap(),
        PathComponentBox::new(&[255]).unwrap(),
        PathComponentBox::new(&[]).unwrap(),
    ])
    .unwrap();

    test_successor(baseline, candidate, max_path);
});
