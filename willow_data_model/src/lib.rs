#![doc(html_logo_url = "https://willowprotocol.org/named_assets/willow_emblem_standalone.png")]
//! # Willow Data Model
//!
//! This crate implements the [Willow Data Model](https://willowprotocol.org/specs/data-model/).
//!
//! [TODO]
//!

mod ordering;
pub use ordering::*;

pub mod paths;

pub mod entry;

/// A “prelude” for crates using the `willow_data_model` crate.
///
/// This prelude is similar to the standard library’s prelude in that you’ll almost always want to import its entire contents, but unlike the standard library’s prelude you’ll have to do so manually:
///
/// `use willow_data_model::prelude::*;`
///
/// The prelude may grow over time.
pub mod prelude {
    pub use super::entry::{Entry, EntryBuilder, Entrylike, EntrylikeExt, Keylike, Timestamp};
    pub use super::ordering::{Maximum, Minimum, Successor};
    pub use super::paths::*;
}
