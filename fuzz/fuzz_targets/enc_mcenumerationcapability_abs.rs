#![no_main]

use meadowcap::{SillyPublicKey, SillySig, UnverifiedMcEnumerationCapability};
use ufotofu_codec::fuzz_absolute_known_size;

fuzz_absolute_known_size!(UnverifiedMcEnumerationCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>);
