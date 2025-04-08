#![no_main]

use meadowcap::{McSubspaceCapability, SillyPublicKey, SillySig};
use ufotofu_codec::fuzz_absolute_all;

fuzz_absolute_all!(McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>);
