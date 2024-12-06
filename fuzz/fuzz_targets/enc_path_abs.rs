#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Path;

const MCL: usize = 300;
const MCC: usize = 300;
const MPL: usize = 300;

fuzz_absolute_all!(Path<MCL,MCC,MPL>);
