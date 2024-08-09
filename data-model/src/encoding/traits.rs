use std::future::Future;

use crate::encoding::error::DecodeError;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// A type that can be encoded to a bytestring, ensuring that any value of `Self` maps to exactly one bytestring.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Encodable {
    /// A function which maps the set `Self` to the set of bytestrings in such a way that any value of `Self` must map to exactly *one* bytestring.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#encode_s)
    fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be decoded from a bytestring, ensuring that every valid encoding maps to exactly one member of `Self`.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Decodable {
    /// A function which maps the set of bytestrings to the set of `T` in such a way that any bytestring must map to exactly *one* value of `Self`.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn decode<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

/// A type that can be used to encode `T` to a bytestring *encoded relative to `R`*.
/// This can be used to create more compact encodings from which `T` can be derived by anyone with `R`.
pub trait RelativeEncodable<R> {
    /// A function which maps the set `(Self, R)` to the set of bytestrings in such a way that any value of `Self` must map to exactly *one* bytestring.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be used to decode `T` from a bytestring *encoded relative to `Self`*.
/// This can be used to decode a compact encoding frow which `T` can be derived by anyone with `R`.
pub trait RelativeDecodable<R> {
    /// A function which maps the set of `(bytestring, R)` to the set of `T` in such a way that any bytestring and reference value must map to exactly *one* value of `Self`.
    fn relative_decode<Producer>(
        reference: &R,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}
