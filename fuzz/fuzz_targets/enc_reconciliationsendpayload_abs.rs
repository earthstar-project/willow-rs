#![no_main]

use ufotofu_codec::fuzz_absolute_known_size;
use wgps::messages::ReconciliationSendPayload;

fuzz_absolute_known_size!(ReconciliationSendPayload);
