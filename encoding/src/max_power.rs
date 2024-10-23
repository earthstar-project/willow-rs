use syncify::syncify;
use syncify::syncify_replace;

/// Returns the least natural number such that 256^`n` is greater than or equal to `n`.
///
/// Used for determining the minimal number of bytes needed to represent a given unsigned integer, and more specifically [`path_length_power`](https://willowprotocol.org/specs/encodings/index.html#path_length_power) and [`path_count_power`](https://willowprotocol.org/specs/encodings/index.html#path_count_power).
pub const fn max_power(max_size: u64) -> u8 {
    if max_size < 256 {
        1
    } else if max_size < 256u64.pow(2) {
        2
    } else if max_size < 256u64.pow(3) {
        3
    } else if max_size < 256u64.pow(4) {
        4
    } else if max_size < 256u64.pow(5) {
        5
    } else if max_size < 256u64.pow(6) {
        6
    } else if max_size < 256u64.pow(7) {
        7
    } else {
        8
    }
}

#[syncify(encoding_sync)]
pub(super) mod encoding {
    use super::*;

    use core::mem::size_of;

    use crate::error::DecodeError;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    /// Encodes a `usize` to a `n`-width unsigned integer, where `n` is the least number of bytes needed to represent `max_size`.
    pub async fn encode_max_power<C>(
        value: usize,
        max_size: usize,
        consumer: &mut C,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        if value > max_size {
            panic!("Can't encode a value larger than its maximum possible value!")
        }

        let power = max_power(max_size as u64);
        let value_encoded_raw: [u8; size_of::<u64>()] = (value as u64).to_be_bytes();

        consumer
            .bulk_consume_full_slice(&value_encoded_raw[size_of::<u64>() - (power as usize)..])
            .await
            .map_err(|f| f.reason)?;

        Ok(())
    }

    /// Decodes a `u64` from `n`-width bytestring, where `n` is the least number of bytes needed to represent `max_size`.
    pub async fn decode_max_power<P>(
        max_size: usize,
        producer: &mut P,
    ) -> Result<u64, DecodeError<P::Error>>
    where
        P: BulkProducer<Item = u8>,
    {
        let power = max_power(max_size as u64);
        let mut slice = [0u8; size_of::<u64>()];

        producer
            .bulk_overwrite_full_slice(&mut slice[size_of::<u64>() - (power as usize)..])
            .await?;

        Ok(u64::from_be_bytes(slice))
    }
}

pub use encoding::{decode_max_power, encode_max_power};
