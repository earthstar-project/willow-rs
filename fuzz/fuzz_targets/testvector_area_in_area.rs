#![no_main]

use ufotofu_codec::fuzz_relative_canonic_corpus;
use willow_25::{SubspaceId25, MCC25, MCL25, MPL25};
use willow_data_model::grouping::Area;

fuzz_relative_canonic_corpus!(Area<MCL25, MCC25, MPL25, SubspaceId25>, Area<MCL25, MCC25, MPL25, SubspaceId25>);
