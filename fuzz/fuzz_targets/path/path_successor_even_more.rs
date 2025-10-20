#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_fuzz::path::*;

fuzz_target!(|data: (PathRc<4, 4, 16>, PathRc<4, 4, 16>)| {
    let (baseline, candidate) = data;
    let max_path = PathRc::from_slices(&[
        &[255, 255, 255, 255].as_slice(),
        &[255, 255, 255, 255].as_slice(),
        &[255, 255, 255, 255].as_slice(),
        &[255, 255, 255, 255].as_slice(),
    ])
    .unwrap();

    test_successor(baseline, candidate, max_path);
});
