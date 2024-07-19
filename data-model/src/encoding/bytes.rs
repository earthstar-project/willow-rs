use either::Either;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::encoding::error::{DecodeError, EncodingConsumerError};

/// Have `Consumer` consume a single byte, or return an [`EncodingConsumerError`] with its `bytes_consumed` field set to `consumed_so_far`.
pub async fn consume_byte<Consumer>(
    byte: u8,
    consumed_so_far: usize,
    consumer: &mut Consumer,
) -> Result<(), EncodingConsumerError<Consumer::Error>>
where
    Consumer: BulkConsumer<Item = u8>,
{
    if let Err(err) = consumer.consume(byte).await {
        return Err(EncodingConsumerError {
            bytes_consumed: consumed_so_far,
            reason: err,
        });
    };

    Ok(())
}

/// Have `Producer` produce a single byte, or return an error if the final value was produced or the producer experienced an error.
pub async fn produce_byte<Producer>(
    producer: &mut Producer,
) -> Result<u8, DecodeError<Producer::Error>>
where
    Producer: BulkProducer<Item = u8>,
{
    match producer.produce().await {
        Ok(Either::Left(item)) => Ok(item),
        Ok(Either::Right(_)) => Err(DecodeError::InvalidInput),
        Err(err) => Err(DecodeError::Producer(err)),
    }
}
