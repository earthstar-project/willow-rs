#![no_main]

use ufotofu_codec::fuzz_absolute_all;

fuzz_absolute_all!(ufotofu_codec_endian::U16BE);
