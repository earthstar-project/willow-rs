use crate::error::DecodeError;
use ufotofu::sync::{BulkConsumer, BulkProducer};

/// A type that can be encoded to a bytestring, ensuring that any value of `Self` maps to exactly one bytestring.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Encodable {
    /// Encode a value to a bytestring in a specific way that is best described over at [willowprotocol.org](https://willowprotocol.org/specs/encodings/index.html#encodings_what).
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#encode_s)
    fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be decoded from a bytestring, ensuring that every valid encoding maps to exactly one member of `Self`.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#encodings_what)
pub trait Decodable {
    /// Decode a value to a bytestring in a specific way that is best described over at [willowprotocol.org](https://willowprotocol.org/specs/encodings/index.html#encodings_what).
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#decode_s)
    fn decode<Producer>(producer: &mut Producer) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

/// A type that can be used to encode `T` to a bytestring *encoded relative to `R`*.
/// This can be used to create more compact encodings from which `T` can be derived by anyone with `R`.
pub trait RelativeEncodable<R> {
    /// Encode a value (relative to a reference value) to a bytestring in a specific way that is best described over at [willowprotocol.org](https://willowprotocol.org/specs/encodings/index.html#encodings_what).
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>;
}

/// A type that can be used to decode `T` from a bytestring *encoded relative to `Self`*.
/// This can be used to decode a compact encoding frow which `T` can be derived by anyone with `R`.
pub trait RelativeDecodable<R> {
    /// Decode a value (relative to a reference value) to a bytestring in a specific way that is best described over at [willowprotocol.org](https://willowprotocol.org/specs/encodings/index.html#encodings_what).
    fn relative_decode<Producer>(
        reference: &R,
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}
