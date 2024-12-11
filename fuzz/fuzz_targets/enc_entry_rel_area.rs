#![no_main]

use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::grouping::Area;
use willow_fuzz::placeholder_params::FakeSubspaceId;

fuzz_relative_all!(Area<16, 16, 16, FakeSubspaceId>; Area<16, 16, 16, FakeSubspaceId>; Blame; Blame);
