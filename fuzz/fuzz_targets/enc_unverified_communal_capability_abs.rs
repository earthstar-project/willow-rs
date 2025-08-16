#![no_main]

use meadowcap::{SillyPublicKey, SillySig, UnverifiedCommunalCapability};
use ufotofu_codec::fuzz_absolute_known_size;

fuzz_absolute_known_size!(UnverifiedCommunalCapability<16, 16, 16, SillyPublicKey, SillyPublicKey, SillySig>);
