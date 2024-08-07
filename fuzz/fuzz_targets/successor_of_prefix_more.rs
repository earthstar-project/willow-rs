#![no_main]

use libfuzzer_sys::fuzz_target;
// use willow_data_model::path::*;
use willow_fuzz::path::*;

// MCL, MCC, MPL
fuzz_target!(|data: (PathRc<3, 3, 3>, PathRc<3, 3, 3>)| {
    let (baseline, candidate) = data;
    let unsucceedables = [
        PathRc::empty(),
        PathRc::new(&[PathComponentBox::new(&[255, 255, 255]).unwrap()]).unwrap(),
        PathRc::new(&[
            PathComponentBox::new(&[255, 255, 255]).unwrap(),
            PathComponentBox::empty(),
        ])
        .unwrap(),
        PathRc::new(&[
            PathComponentBox::new(&[255, 255, 255]).unwrap(),
            PathComponentBox::empty(),
            PathComponentBox::empty(),
        ])
        .unwrap(),
    ];

    test_greater_but_not_prefixed(baseline, candidate, &unsucceedables);
});
