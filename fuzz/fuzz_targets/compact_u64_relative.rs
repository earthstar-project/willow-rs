#![no_main]

use core::convert::Infallible;

use compact_u64::NotMinimal;
use ufotofu_codec::fuzz_relative_all;

fuzz_relative_all!(compact_u64::CompactU64; compact_u64::Tag; Infallible; NotMinimal);
