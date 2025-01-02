#![no_main]

use ufotofu_codec::{fuzz_relative_all, Blame};
use willow_data_model::grouping::Range3d;
use willow_fuzz::placeholder_params::FakeSubspaceId;

fuzz_relative_all!(Range3d<16, 16, 16, FakeSubspaceId>; Range3d<16, 16, 16, FakeSubspaceId>; Blame; Blame);
