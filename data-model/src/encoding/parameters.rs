use std::future::Future;

use crate::encoding::error::{DecodeError, EncodingConsumerError};
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

pub trait Encoder<Consumer>
where
    Consumer: BulkConsumer<Item = u8>,
{
    /// A function from the set `Self` to the set of bytestrings, ensuring that any value of `Self` maps to exactly one bytestring.
    fn encode(
        &self,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), EncodingConsumerError<Consumer::Error>>>;
}

pub trait Decoder<Producer>
where
    Producer: BulkProducer<Item = u8>,
    Self: Sized,
{
    /// A function from the set of bytestrings to the set of `T`, ensuring that every valid encoding maps to exactly one member of `T`.
    fn decode(
        producer: &mut Producer,
    ) -> impl Future<Output = Result<Self, DecodeError<Producer::Error>>>;
}
