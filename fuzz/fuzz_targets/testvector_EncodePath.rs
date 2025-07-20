#![no_main]

use ufotofu_codec::fuzz_absolute_corpus;
use willow_data_model::Path;

const MCL: usize = 1024;
const MCC: usize = 1024;
const MPL: usize = 1024;

fuzz_absolute_corpus!(Path<MCL,MCC,MPL>);
