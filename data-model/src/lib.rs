//! # Willow Data Model
//!
//! This crate provides implementation of the [Willow Data Model](https://willowprotocol.org/specs/data-model/index.html#data_model), including:
//!
//! - Traits to assist in your implementation of Willow [parameters](https://willowprotocol.org/specs/data-model/index.html#willow_parameters), such as [`NamespaceId`](https://willowprotocol.org/specs/data-model/index.html#NamespaceId) and [`SubspaceId`](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).  
//! - A [zero-copy](https://en.wikipedia.org/wiki/Zero-copy) implementation of Willow [paths](https://willowprotocol.org/specs/data-model/index.html#Path) and their constituent [components](https://willowprotocol.org/specs/data-model/index.html#Component).
//! - An implementation of Willow's [entries](https://willowprotocol.org/specs/data-model/index.html#Entry).
//! - Utilities for Willow's entry [groupings](https://willowprotocol.org/specs/grouping-entries/index.html#grouping_entries), such as [ranges](https://willowprotocol.org/specs/grouping-entries/index.html#ranges) and [areas](https://willowprotocol.org/specs/grouping-entries/index.html#areas)
//! - Utilities for encoding and decoding, as well as implementations of all of Willow's [encodings](https://willowprotocol.org/specs/encodings/index.html#encodings).
//!
//! This crate **does not yet have** anything for Willow's concept of [stores](https://willowprotocol.org/specs/data-model/index.html#store). Stay tuned!

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
