use std::future::Future;

use crate::encoding::error::{DecodeError, EncodingConsumerError};
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

pub trait Encoder {
    /// A function from the set `Self` to the set of bytestrings, ensuring that any value of `Self` maps to exactly one bytestring.
    fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), EncodingConsumerError<Consumer::Error>>>
    where
        Consumer: BulkConsumer<Item = u8>;
}

pub trait Decoder {
    /// A function from the set of bytestrings to the set of `T`, ensuring that every valid encoding maps to exactly one member of `T`.
    fn decode<Producer>(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}
