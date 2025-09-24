#![no_main]

use meadowcap::{SillyPublicKey, SillySig, UnverifiedMcCapability};
use ufotofu_codec::fuzz_absolute_known_size;

fuzz_absolute_known_size!(UnverifiedMcCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>);
