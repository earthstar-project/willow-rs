#![no_main]

use libfuzzer_sys::fuzz_target;
// use willow_data_model::path::*;
use willow_data_model_fuzz::path::*;

// MCL, MCC, MPL
fuzz_target!(|data: (PathRc<4, 4, 16>, PathRc<4, 4, 16>)| {
    let (baseline, candidate) = data;
    let unsucceedables = [
        PathRc::empty(),
        PathRc::new(&[PathComponentBox::new(&[255, 255, 255, 255]).unwrap()]).unwrap(),
        PathRc::new(&[
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        ])
        .unwrap(),
        PathRc::new(&[
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        ])
        .unwrap(),
        PathRc::new(&[
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
            PathComponentBox::new(&[255, 255, 255, 255]).unwrap(),
        ])
        .unwrap(),
    ];

    test_successor_of_prefix(baseline, candidate, &unsucceedables);
});
