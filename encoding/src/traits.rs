use std::future::Future;

use crate::error::DecodeError;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// A type which can be asynchronously encoded to bytes consumed by a [`ufotofu::local_nb::BulkConsumer`]
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Encodable {
    /// Encode the value with an [encoding function](https://macromania--channelclosing.deno.dev/specs/encodings/index.html#encoding_function) for the set of `Self`, ensuring that any value of `Self` maps to exactly one bytestring, such that there exists a _decoding_ function such that:
    ///
    /// - for every value `s` in `Self` and every bytestring `b` that starts with `encode_s(s)`, we have `decode_s(b)=s`, and
    /// - for every `s` in `Self` and every bytestring b that does not start with `encode_s(s)`, we have `decode_s(b) ≠ s`.
    ///
    /// The encoded bytestring will be consumed to the provided [`ufotofu::local_nb::BulkConsumer`].
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#encode_s)
    fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type which can be asynchronously decoded from bytes produced by a [`ufotofu::local_nb::BulkProducer`]
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Decodable {
    /// Decode a bytestring created with an [encoding function](https://macromania--channelclosing.deno.dev/specs/encodings/index.html#encoding_function) for the set of `Self`, ensuring that any value of `Self` maps to exactly one bytestring, such that there exists a _decoding_ function (provided by this trait!) such that:
    ///
    /// - for every value `s` in `Self` and every bytestring `b` that starts with `encode_s(s)`, we have `decode_s(b)=s`, and
    /// - for every `s` in `Self` and every bytestring `b` that does not start with `encode_s(s)`, we have `decode_s(b) ≠ s`.
    ///
    /// Will return an error if the encoding correlates to the **encoding relation** and not the result of the aforementioned encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn decode<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decode a bytestring belonging to the **encoding relation** on the set of `Self` and the set of bytestrings, such that:
    ///
    /// - for every `s` in `Self`, there is at least one bytestring in relation with `s`, and
    /// - no bytestring in the relation is a prefix of another bytestring in the relation.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn decode_relation<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

/// A type relative to a reference value of type `R` which can be asynchronously encoded to bytes consumed by a [`ufotofu::local_nb::BulkConsumer`]
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait RelativeEncodable<R> {
    /// Encode a pair of `Self` and `R` with the [encoding function](https://macromania--channelclosing.deno.dev/specs/encodings/index.html#encoding_function) for the set of `Self` relative to a value of the set of `R`, ensuring that any pair of `Self` and `R` maps to exactly one bytestring, such that there exists a _decoding_ function such that:
    ///
    /// - for every pair of `s` in `Self` and `r` in `R`, and every bytestring `b` that starts with `encode_s(s, r)`, we have `decode_s(b, r)=s`, and
    /// - for every pair of `s` in `Self` and `r` in `R`, and every bytestring `b` that does not start with `encode_s(s, r)`, we have `decode_s(b, r) ≠ s`.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type relative to a value of type `R` which can be asynchronously decoded from bytes produced by a [`ufotofu::local_nb::BulkProducer`]
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait RelativeDecodable<R> {
    ///
    /// Decode a bytestring created with an [encoding function](https://macromania--channelclosing.deno.dev/specs/encodings/index.html#encoding_function) for the set of `Self` relative to a value of the set of `R`, ensuring that any pair of `Self` and `R` maps to exactly one bytestring, such that there exists a _decoding_ function (provided by this trait!) such that:
    ///
    /// - for every pair of `s` in `Self` and `r` in `R`, and every bytestring `b` that starts with `encode_s(s, r)`, we have `decode_s(b, r)=s`, and
    /// - for every pair of `s` in `Self` and `r` in `R`, and every bytestring `b` that does not start with `encode_s(s, r)`, we have `decode_s(b, r) ≠ s`.
    ///
    /// Will return an error if the encoding correlates to the **encoding relation** and not the result of the aforementioned encoding function.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn relative_decode<Producer>(
        reference: &R,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;

    /// Decode a bytestring belonging to the **encoding relation** on the set of `Self` relative to a value of the set of `R`, and the set of bytestrings, such that:
    ///
    /// - for every pair of `s` in `Self` and `r` in `R`, there is at least one bytestring in relation with the pair of `s` and `r`, and
    /// - no bytestring in the relation is a prefix of another bytestring in the relation.
    fn relative_decode_relation<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}
