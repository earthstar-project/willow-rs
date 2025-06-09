//! # Willowʼ25
//!
//! This crate provides concrete parameter choices for the Willow family of specifications, currently:
//!
//! - [Willow Data Model](https://willowprotocol.org/specs/data-model/index.html#data_model)
//! - [Meadowcap](https://willowprotocol.org/specs/meadowcap/index.html#meadowcap)
//!
//! With choices for the [Willow Sideloading Protocol](https://willowprotocol.org/specs/sideloading/index.html#sideloading) and [Willow General Purpose Sync Protocol](https://willowprotocol.org/specs/sync/index.html#sync) to follow.
//!
//! We want it to be easy (and safe!) to start using the various Willow crates, and for there to be an ecosystem of interoperable Willow instances. We'd also like to release this in the year 2025 C.E. Hence, Willowʼ25!
//!
//! Willowʼ25 uses:
//!
//! - ed25519 signing and verification provided by [`ed25519_dalek`](https://docs.rs/ed25519-dalek/latest/ed25519_dalek/)
//! - (Temporarily) BLAKE3 cryptographic hashing provided by [`blake3`](https://docs.rs/blake3/latest/blake3/). This will eventually be replaced by WILLAM3 cryptographic hashing to allow partial payload verification.
//!
//! These choices have been chosen to balance performance and cryptographic security. They have **not** yet been audited in combination with Willow protocols.

/// A usize for the Willow Data Model's [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_component_length) parameter.
pub const MCL25: usize = 1024;
/// A usize for the Willow Data Model's [`max_component_count`](https://willowprotocol.org/specs/data-model/index.html#max_component_count) parameter.
pub const MCC25: usize = 1024;
/// A usize for the Willow Data Model's [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_path_length) parameter.
pub const MPL25: usize = 1024;

mod namespace;
pub use namespace::*;

mod subspace;
pub use subspace::*;

mod payload_digest;
pub use payload_digest::*;

mod authorisation_token;
pub use authorisation_token::*;

mod signature;
pub use signature::*;

pub mod data_model;
pub use data_model::{
    Area, AreaOfInterest, AuthorisedEntry, Component, Entry, LengthyAuthorisedEntry, LengthyEntry,
    OwnedComponent, Path, Range, Range3d, Timestamp,
};
