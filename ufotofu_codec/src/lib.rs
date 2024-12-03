#![no_std]

//! # UFOTOFU Codec
//!
//! Traits for (streaming) encoding and decoding of values to and from bytestrings using [UFOTOFU](https://crates.io/crates/ufotofu).
//!
//! Intuitively, we request the following properties of encodings:
//!
//! - each value has at least one encoding (but possibly more),
//! - no two values have the same encoding, and
//! - no encoding is a prefix of another encoding.
//!
//! Formally, we say an *encoding relation* is a set of pairs (i.e., a [binary relation](https://en.wikipedia.org/wiki/Binary_relation)) `(t, enc)` of a value of type `T` and a finite bytestring, such that
//!
//! - for each `t` in `T` there is at least one pair in the relation that contains `t` (i.e., the relation is a [total function](https://en.wikipedia.org/wiki/Function_(mathematics))),
//! - there are no two distinct `t1, t2` in `T` such that there is a single encoding `enc` such that `(t1, enc)` and `(t2, enc)` are both in the relation (i.e., the relation is [injective](https://en.wikipedia.org/wiki/Injective_function)), and
//! - for any two bytestrings `enc1, enc2` in the relation, neither is a strict prefix of the other (i.e., we have a [prefix code](https://en.wikipedia.org/wiki/Prefix_code)).
//!
//! ## Traits
//!
//! The [`Encodable`] and [`Decodable`] trait describe how to convert between the domain of encodable values and the domain of finite bytestrings. Further traits specialise these:
//!
//! - [`EncodableCanonic`] and [`DecodableCanonic`] allow working with canonic subsets of an encoding relation, i.e., encodings where there is a one-to-one correspondence between values and codes.
//! - [`EncodableSync`] and [`DecodableSync`] are marker traits that signal that all asynchrony in encoding/decoding stems from the consumer/producer, but all further computations are synchronous. When using a consumer/producer that never blocks either, this allows for synchronous encoding/decoding.
//! - [`EncodableKnownSize`] describes how to compute the size of an encoding without actually performing the encoding. Useful when the size needs to be known in advance (length-prefixing, or encoding into a slice).
//!
//! ## Relativity
//!
//! All traits support encoding and decoding *relative* to some reference value that is supplied dynamically at runtime. For absolute encodings, set the type of the reference value to `()`; this enables convenience methods that do not require passing a `&()` all the time.
//!
//! Formally, each possible reference value defines an encoding relation that is independent from the relations defined by any other reference values. All API contracts that are stated in the documentation must hold for the same reference value. As an example, consider an encoding for integers that encodes relative to a reference value by encoding the difference between the value to encode and the reference value. Then, when encoding any two distinct values relative to the reference value `5`, the resulting encodings must be different. But when encoding some value `x` relative to `5` and another value `y` relative to `17`, then it is perfectly fine for both encodings to be the same bytestring.
//!
//! ## Encoders and Decoders
//!
//! TODO
//!
//! ## Property Testing
//!
//! When the `dev` feature is enabled, the [`proptest`] module provides helpers for writing property tests for checking the invariants that implementations of the traits of this crate must uphold.
//!
//! [TODO] check all docs, update to the most recent refactorings...

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "dev")]
pub mod proptest;

mod decode_error;
pub use decode_error::DecodeError;

mod decode;
pub use decode::*;

mod relative_decode;
pub use relative_decode::*;

mod encode;
pub use encode::*;

mod relative_encode;
pub use relative_encode::*;

// TODO Encoder, Decoder, DecoderCanonic; absolute and relative; (owned and borrowed for the relative ones?)

// TODO proptest stuff