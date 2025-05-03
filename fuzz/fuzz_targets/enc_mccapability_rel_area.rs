#![no_main]

use meadowcap::{McCapability, SillyPublicKey, SillySig};
use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::grouping::Area;

fuzz_relative_all!(McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>; Area<16, 16, 16, SillyPublicKey>; Blame; Blame; |cap: &McCapability<16, 16, 16, SillyPublicKey, SillySig, SillyPublicKey, SillySig>, area: &Area<16, 16, 16, SillyPublicKey>| {
    area.includes_area(&cap.granted_area())
});
