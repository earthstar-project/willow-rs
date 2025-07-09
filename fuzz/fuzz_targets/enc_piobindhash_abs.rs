#![no_main]

use ufotofu_codec::fuzz_absolute_all;
use wgps::PioBindHash;

fuzz_absolute_all!(Path<MCL,MCC,MPL>);
