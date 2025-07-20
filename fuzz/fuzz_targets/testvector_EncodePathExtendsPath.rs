#![no_main]

use libfuzzer_sys::fuzz_target;

use ufotofu::producer::FromSlice;
use willow_data_model::{decode_path_extends_path, Path};

const MCL: usize = 1024;
const MCC: usize = 1024;
const MPL: usize = 1024;

fuzz_target!(|data: (Box<[u8]>, Path<MCL, MCC, MPL>)| {
    let (bytes, r) = data;

    pollster::block_on(async {
        let _decoded = decode_path_extends_path(&mut FromSlice::new(&bytes[..]), &r).await;
    });
});
