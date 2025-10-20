mod keylike;
pub use keylike::*;

mod entry;
pub use entry::*;

/// A Timestamp is a 64-bit unsigned integer, that is, a natural number between zero (inclusive) and 2^64 (exclusive).
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#Timestamp).
pub type Timestamp = u64;
