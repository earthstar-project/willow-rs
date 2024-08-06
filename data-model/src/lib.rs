//! Allo!

#![feature(
    new_uninit,
    async_fn_traits,
    debug_closure_helpers,
    maybe_uninit_uninit_array,
    maybe_uninit_write_slice,
    maybe_uninit_slice
)]

pub mod encoding;
mod entry;
pub use entry::*;
pub mod grouping;

mod parameters;
pub use parameters::*;

mod path;
pub use path::*;
