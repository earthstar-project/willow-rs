#![no_main]

use meadowcap::{SillyPublicKey, SillySig, UnverifiedOwnedCapability};
use ufotofu_codec::fuzz_absolute_known_size;

fuzz_absolute_known_size!(UnverifiedOwnedCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>);
