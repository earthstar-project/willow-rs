#![no_main]

use ufotofu_codec::fuzz_absolute_known_size;
use wgps::messages::PioBindHash;

fuzz_absolute_known_size!(PioBindHash<8>);
