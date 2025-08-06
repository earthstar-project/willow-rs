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

pub const DEFAULT_SIGNING_KEY: [u8; 32] = [
    94, 20, 172, 228, 210, 200, 2, 143, 200, 154, 143, 4, 118, 91, 25, 210, 205, 117, 45, 145, 187,
    55, 60, 12, 158, 212, 118, 39, 107, 92, 69, 65,
];

pub const DEFAULT_PUBLIC_KEY: [u8; 32] = [
    147, 78, 96, 33, 51, 158, 31, 1, 59, 169, 73, 0, 237, 194, 93, 141, 116, 192, 180, 229, 115,
    118, 137, 16, 174, 15, 80, 125, 140, 129, 115, 24,
];

mod namespace;
pub use namespace::*;

mod subspace;
pub use subspace::*;

mod payload_digest;
pub use payload_digest::*;

mod signature;
pub use signature::*;

pub mod data_model;
pub use data_model::{
    Area, AreaOfInterest, AreaSubspace, AuthorisedEntry, Component, Entry, LengthyAuthorisedEntry,
    LengthyEntry, OwnedComponent, Path, Range, Range3d, Timestamp,
};

pub mod meadowcap;
pub use meadowcap::{
    AccessMode, AuthorisationToken, Capability, CommunalCapability, OwnedCapability,
};

pub mod sideload;
pub use sideload::{create_drop, ingest_drop};
