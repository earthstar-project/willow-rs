#![no_main]

use ufotofu_codec::fuzz_relative_corpus;
use willow_25::{NamespaceId25, PayloadDigest25, SubspaceId25, MCC25, MCL25, MPL25};
use willow_data_model::{grouping::Range3d, Entry};

fuzz_relative_corpus!(Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>, (NamespaceId25, Range3d<MCL25, MCC25, MPL25, SubspaceId25>));
