#![doc(html_logo_url = "https://willowprotocol.org/named_assets/willow_emblem_standalone.png")]
//! # Willow Data Model
//!
//! This crate provides implementation of the [Willow Data Model](https://willowprotocol.org/specs/data-model/index.html#data_model), including:
//!
//! - Traits to assist in your implementation of Willow [parameters](https://willowprotocol.org/specs/data-model/index.html#willow_parameters), such as [`NamespaceId`](https://willowprotocol.org/specs/data-model/index.html#NamespaceId) and [`SubspaceId`](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).  
//! - A [zero-copy](https://en.wikipedia.org/wiki/Zero-copy) implementation of Willow [paths](https://willowprotocol.org/specs/data-model/index.html#Path) and their constituent [components](https://willowprotocol.org/specs/data-model/index.html#Component).
//! - An implementation of Willow's [entries](https://willowprotocol.org/specs/data-model/index.html#Entry).
//! - Utilities for Willow's entry [groupings](https://willowprotocol.org/specs/grouping-entries/index.html#grouping_entries), such as [ranges](https://willowprotocol.org/specs/grouping-entries/index.html#ranges) and [areas](https://willowprotocol.org/specs/grouping-entries/index.html#areas)
//! - Implementations of various [relative encodings](https://willowprotocol.org/specs/encodings/index.html#relativity).
//!
//! This crate **does not yet have** anything for Willow's concept of [stores](https://willowprotocol.org/specs/data-model/index.html#store). Stay tuned!
//!
//! ## Type parameters
//!
//! Willow is a parametrised family of protocols, and so this crate makes heavy use of generic parameters.
//!
//! The following generic parameter names are used consistently across this crate:
//!
//! - `MCL` - A `usize` representing [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_component_length).
//! - `MCC` - A `usize` representing [`max_component_count`](https://willowprotocol.org/specs/data-model/index.html#max_component_count).
//! - `MPL` - A `usize` representing [`max_path_length`](https://willowprotocol.org/specs/data-model/index.html#max_path_length).
//! - `N` - The type used for [`NamespaceId`](https://willowprotocol.org/specs/data-model/index.html#NamespaceId) (willowprotocol.org), must implement the [`NamespaceId`] trait.
//! - `S` - The type used for [`SubspaceId`](https://willowprotocol.org/specs/data-model/index.html#SubspaceId) (willowprotocol.org), must implement the [`SubspaceId`] trait.
//! - `PD` - The type used for [`PayloadDigest`](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest) (willowprotocol.org), must implement the [`PayloadDigest`] trait.
//! - `AT` - The type used for [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) (willowprotocol.org), must implement the [`AuthorisationToken`] trait.

mod entry;
pub use entry::*;
pub mod grouping;
mod parameters;
pub use parameters::*;
mod path;
pub use path::*;
mod relative_encodings;
mod store;
pub use store::*;
