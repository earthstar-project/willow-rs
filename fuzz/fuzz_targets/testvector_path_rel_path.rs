#![no_main]

use ufotofu_codec::fuzz_relative_canonic_corpus;
use willow_data_model::Path;

const MCL: usize = 1024;
const MCC: usize = 1024;
const MPL: usize = 1024;

fuzz_relative_canonic_corpus!(Path<MCL,MCC,MPL>, Path<MCL,MCC,MPL>);
