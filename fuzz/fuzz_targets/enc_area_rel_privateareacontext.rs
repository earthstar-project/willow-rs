#![no_main]

use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::grouping::Area;
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakeSubspaceId};
use willow_pio::PrivateAreaContext;

fuzz_relative_basic!(
    Area<16, 16, 16, FakeSubspaceId>;
    PrivateAreaContext<16, 16, 16, FakeNamespaceId, FakeSubspaceId>;
    Blame;
    |area :  &Area<16, 16, 16, FakeSubspaceId>,  private_area:  &PrivateAreaContext<16, 16, 16, FakeNamespaceId, FakeSubspaceId>| {
private_area.rel().almost_includes_area(area) && private_area.private().almost_includes_area(area)
    }
);
