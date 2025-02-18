#![no_main]

use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::{grouping::Area, PrivateAreaContext};
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakeSubspaceId};

fuzz_relative_basic!(
    Area<16, 16, 16, FakeSubspaceId>;
    PrivateAreaContext<16, 16, 16, FakeNamespaceId, FakeSubspaceId>;
    Blame;
    |area :  &Area<16, 16, 16, FakeSubspaceId>,  private_area:  &PrivateAreaContext<16, 16, 16, FakeNamespaceId, FakeSubspaceId>| {
private_area.private().almost_includes_area(area) && private_area.rel().includes_area(area)
    }
);
