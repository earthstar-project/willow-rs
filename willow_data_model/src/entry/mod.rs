mod keylike;
pub use keylike::*;

mod entry;
pub use entry::*;

/// A type of Willow [PayloadDigests](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest), used for [content-addressing](https://en.wikipedia.org/wiki/Content_addressing) the [Payloads](https://willowprotocol.org/specs/data-model/index.html#Payload) that Willow stores.
///
/// This trait primarily describes how to compute the digest of any payload by feeding successive slices into a [`GenericHasher`].
///
/// Further, this trait extends [`Ord`], because Willow mandates PayloadDigests to be totally ordered.
pub trait PayloadDigest: Ord {
    /// The state for hashing slices of bytes into PayloadDigests.
    type Hasher: GenericHasher<Digest = Self>;

    /// Returns a new initial state for computing digests.
    fn hasher() -> Self::Hasher;
}

/// A trait just like [`std::hash::Hasher`], except the type of digests is specified as an associated type (instead of being hardcoded to [`u64`]).
pub trait GenericHasher {
    /// The type of digests produced by this hasher.
    type Digest;

    /// Returns the digest for the values written so far.
    ///
    /// Despite its name, the method does not reset the hasher’s internal state. Additional writes will continue from the current value. If you need to start a fresh hash value, you will have to create a new hasher.
    fn finish(&self) -> Self::Digest;

    /// Writes some data into the given Hasher.
    fn write(&mut self, bytes: &[u8]);
}

/// A Timestamp is a 64-bit unsigned integer, that is, a natural number between zero (inclusive) and 2^64 (exclusive).
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#Timestamp).
pub type Timestamp = u64;
