#![no_main]

use ufotofu_codec::fuzz_relative_corpus;
use willow_25::{NamespaceId25, PayloadDigest25, SubspaceId25, MCC25, MCL25, MPL25};
use willow_data_model::Entry;

fuzz_relative_corpus!(Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>, Entry<MCL25, MCC25, MPL25, NamespaceId25, SubspaceId25, PayloadDigest25>);
