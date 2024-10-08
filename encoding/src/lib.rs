//! Utilities for implementing Willow's various [encodings](https://willowprotocol.org/specs/encodings/index.html#encodings).

mod bytes;
mod compact_width;
mod error;
mod max_power;
mod traits_sync;
mod unsigned_int;

pub use bytes::encoding::produce_byte;
pub use bytes::is_bitflagged;

pub use compact_width::encoding::*;
pub use compact_width::CompactWidth;

pub use error::*;

mod traits;
pub use traits::*;

pub use unsigned_int::*;

pub use max_power::{decode_max_power, encode_max_power, max_power};

pub mod sync {
    //! Synchronous variants of utilities for implementing Willow's various [encodings](https://willowprotocol.org/specs/encodings/index.html#encodings).

    pub use super::bytes::encoding_sync::produce_byte;
    pub use super::compact_width::encoding_sync::*;
    pub use super::max_power::encoding_sync::*;
    pub use super::traits_sync::*;
}
