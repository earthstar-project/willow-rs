//! Utilities for implementing Willow's various [encodings](https://willowprotocol.org/specs/encodings/index.html#encodings).

mod bytes;
mod compact_width;
mod error;
mod max_power;
mod unsigned_int;

pub use compact_width::encoding::*;
pub use compact_width::CompactWidth;

pub use error::*;

mod traits;
pub use traits::*;

pub use unsigned_int::*;

pub use max_power::{decode_max_power, encode_max_power, max_power};

use either::Either::*;
use ufotofu::BulkProducer;

/// Returns whether a bit at the given position is `1` or not. Position `0` is the most significant bit, position `7` the least significant bit.
pub fn is_bitflagged(byte: u8, position: u8) -> bool {
    let mask = 1 << (7 - position);
    byte & mask == mask
}

/// Produce exactly one byte, or return a [`DecodeError`].
pub async fn produce_byte<Producer>(
    producer: &mut Producer,
) -> Result<u8, DecodeError<Producer::Error>>
where
    Producer: BulkProducer<Item = u8>,
{
    match producer.produce().await {
        Ok(Left(item)) => Ok(item),
        Ok(Right(_)) => Err(DecodeError::InvalidInput),
        Err(err) => Err(DecodeError::Producer(err)),
    }
}
