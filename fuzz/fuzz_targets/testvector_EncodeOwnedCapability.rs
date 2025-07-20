#![no_main]

use meadowcap::UnverifiedOwnedCapability;
use ufotofu_codec::fuzz_absolute_corpus;
use willow_25::{NamespaceId25, Signature25, SubspaceId25, MCC25, MCL25, MPL25};

fuzz_absolute_corpus!(UnverifiedOwnedCapability<MCL25, MCC25, MPL25, NamespaceId25, Signature25, SubspaceId25, Signature25>);
