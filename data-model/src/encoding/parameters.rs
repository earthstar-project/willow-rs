use crate::encoding::error::{DecodeError, EncodingConsumerError};
use core::ops::AsyncFn;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// A function from the set `T` to the set of bytestrings, ensuring that any value of `T` maps to exactly one bytestring.
pub trait EncodingFunction<T, Consumer>:
    AsyncFn(&T, &mut Consumer) -> Result<(), EncodingConsumerError<Consumer::Error>>
where
    Consumer: BulkConsumer<Item = u8>,
{
}

/// A function from the set of bytestrings to the set of `T`, ensuring that every valid encoding maps to exactly one member of `T`.
pub trait DecodingFunction<T, Producer>:
    AsyncFn(&mut Producer) -> Result<T, DecodeError<Producer::Error>>
where
    Producer: BulkProducer<Item = u8>,
{
}
