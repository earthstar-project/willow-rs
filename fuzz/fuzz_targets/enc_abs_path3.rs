#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Path;

const MCL: usize = 4;
const MCC: usize = 4;
const MPL: usize = 6;

fuzz_absolute_all!(Path<MCL,MCC,MPL>);
