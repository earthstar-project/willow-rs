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

use core::convert::Infallible;
use core::fmt::{Debug, Display, Formatter};
use core::future::Future;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
#[cfg(all(feature = "std", not(feature = "alloc")))]
use std::{boxed::Box, collections::Vec};

#[cfg(feature = "std")]
use std::error::Error;

use either::Either::*;
use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, OverwriteFullSliceError,
};

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

// /// Methods for encoding a value that belongs to an *encoding relation*.
// ///
// /// API contracts:
// ///
// /// - The encoding must not depend on details of the consumer such as when it yields or how many item slots it exposes at a time.
// /// - Nonequal values must result in nonequal encodings.
// /// - No encoding must be a prefix of a different encoding.
// /// - For types that also implement `Decodable` and `Eq`, encoding a value and then decoding it must yield a value equal to the original.
// pub trait Encodable {
//     /// The type of values relative to which we encode. Set to `()` for absolute encodings.
//     type EncodeRelativeTo;

//     /// Writes an encoding of `&self` into the given consumer.
//     fn relative_encode<C>(
//         &self,
//         consumer: &mut C,
//         r: &Self::EncodeRelativeTo,
//     ) -> impl Future<Output = Result<(), C::Error>>
//     where
//         C: BulkConsumer<Item = u8>;

//     /// Writes an absolute encoding of `&self` into the given consumer.
//     fn encode<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
//     where
//         C: BulkConsumer<Item = u8>,
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         self.relative_encode(consumer, &())
//     }

//     #[cfg(feature = "alloc")]
//     /// Encodes into a Vec instead of a given consumer.
//     fn relative_encode_into_vec(
//         &self,
//         r: &Self::EncodeRelativeTo,
//     ) -> impl Future<Output = Vec<u8>> {
//         async {
//             let mut c = IntoVec::new();

//             match self.relative_encode(&mut c, r).await {
//                 Ok(()) => c.into_vec(),
//                 Err(_) => unreachable!(),
//             }
//         }
//     }

//     #[cfg(feature = "alloc")]
//     /// Absolutely encodes into a Vec instead of a given consumer.
//     fn encode_into_vec(&self) -> impl Future<Output = Vec<u8>>
//     where
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         self.relative_encode_into_vec(&())
//     }
// }

// /// Encodables that can (efficiently and synchronously) precompute the length of their encoding.
// ///
// /// API contract: `self.encode(c)` must write exactly `self.len_of_encoding()` many bytes into `c`.
// pub trait EncodableKnownSize: Encodable {
//     /// Computes the size of the encoding in bytes. Calling [`relative_encode`](Encodable::relative_encode) must feed exactly that many bytes into the consumer.
//     fn relative_len_of_encoding(&self, r: &Self::EncodeRelativeTo) -> usize;

//     /// Computes the size of the absolute encoding in bytes. Calling [`encode`](Encodable::encode) must feed exactly that many bytes into the consumer.
//     fn len_of_encoding(&self) -> usize
//     where
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         self.relative_len_of_encoding(&())
//     }

//     #[cfg(feature = "alloc")]
//     /// Encodes into a boxed slice instead of a given consumer.
//     fn relative_encode_into_boxed_slice(
//         &self,
//         r: &Self::EncodeRelativeTo,
//     ) -> impl Future<Output = Box<[u8]>> {
//         async {
//             let mut c = IntoVec::with_capacity(self.relative_len_of_encoding(r));

//             match self.relative_encode(&mut c, r).await {
//                 Ok(()) => c.into_vec().into_boxed_slice(),
//                 Err(_) => unreachable!(),
//             }
//         }
//     }

//     #[cfg(feature = "alloc")]
//     /// Absolutely encodes into a boxed slice instead of a given consumer.
//     fn encode_into_boxed_slice(&self) -> impl Future<Output = Box<[u8]>>
//     where
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         self.relative_encode_into_boxed_slice(&())
//     }
// }

// /// An encodable that introduces no asynchrony beyond that of `.await`ing the consumer.
// ///
// /// If the consumer is known to not block either, this enables synchronous encoding.
// pub trait EncodableSync: Encodable {
//     #[cfg(feature = "alloc")]
//     /// Synchronously encodes into a Vec instead of a given consumer.
//     fn sync_relative_encode_into_vec(&self, r: &Self::EncodeRelativeTo) -> Vec<u8> {
//         pollster::block_on(self.relative_encode_into_vec(r))
//     }

//     /// Synchronously and absolutely encodes into a Vec instead of a given consumer.
//     fn sync_encode_into_vec(&self) -> Vec<u8>
//     where
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         pollster::block_on(self.encode_into_vec())
//     }

//     #[cfg(feature = "alloc")]
//     /// Synchronously encodes into a boxed slice instead of a given consumer.
//     fn sync_relative_encode_into_boxed_slice(&self, r: &Self::EncodeRelativeTo) -> Box<[u8]>
//     where
//         Self: EncodableKnownSize,
//     {
//         pollster::block_on(self.relative_encode_into_boxed_slice(r))
//     }

//     #[cfg(feature = "alloc")]
//     /// Synchronously and absolutely encodes into a boxed slice instead of a given consumer.
//     fn sync_encode_into_boxed_slice(&self) -> Box<[u8]>
//     where
//         Self: EncodableKnownSize,
//         Self: Encodable<EncodeRelativeTo = ()>,
//     {
//         pollster::block_on(self.encode_into_boxed_slice())
//     }
// }

// // TODO Encoder, Decoder, DecoderCanonic; absolute and relative; (owned and borrowed for the relative ones?)

// // TODO proptest stuff
