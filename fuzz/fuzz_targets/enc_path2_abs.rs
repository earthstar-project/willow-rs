#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Path;

const MCL: usize = 2;
const MCC: usize = 3;
const MPL: usize = 3;

fuzz_absolute_all!(Path<MCL,MCC,MPL>);
