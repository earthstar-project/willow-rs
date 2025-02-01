#![no_main]

use meadowcap::McSubspaceCapability;
use ufotofu_codec::fuzz_absolute_all;
use willow_data_model::Path;
use willow_fuzz::silly_sigs::{SillyPublicKey, SillySig};

fuzz_absolute_all!(McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>);
