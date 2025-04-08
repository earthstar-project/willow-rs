#![no_main]

use ufotofu_codec::{fuzz_relative_basic, Blame};
use willow_data_model::{Path, PrivatePathContext};

fuzz_relative_basic!(
    Path<16, 16, 16>;
    PrivatePathContext<16, 16, 16>;
    Blame;
    |path :  &Path<16, 16, 16>,  private_path:  &PrivatePathContext<16, 16, 16>| {
private_path.rel().is_prefix_of(path) && private_path.private().is_related(path)
    }
);
