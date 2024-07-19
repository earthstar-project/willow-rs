use std::future::Future;

use crate::encoding::error::{DecodeError, EncodingConsumerError};
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// A type that can be encoded to a bytestring, ensuring that any value of `Self` maps to exactly one bytestring.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Encodable {
    /// A function from the set `Self` to the set of bytestrings.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#encode_s)
    fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), EncodingConsumerError<Consumer::Error>>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be decoded from a bytestring, ensuring that every valid encoding maps to exactly one member of `Self`.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Decodable {
    /// A function from the set of bytestrings to the set of `T`.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn decode<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}
