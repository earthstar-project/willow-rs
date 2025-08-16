#![no_main]

use meadowcap::UnverifiedMcEnumerationCapability;
use ufotofu_codec::fuzz_absolute_corpus;
use willow_25::{NamespaceId25, Signature25, SubspaceId25};

fuzz_absolute_corpus!(UnverifiedMcEnumerationCapability<NamespaceId25, Signature25, SubspaceId25, Signature25>);
