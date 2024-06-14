// Encoders and decoders.

use core::mem::MaybeUninit;

use std::fmt::Debug;

use thiserror::Error;
use ufotofu::sync::{self, BulkConsumer, BulkProducer};

/// Trait for encoding values into bytes.
pub trait Encoder: Debug {
    fn encode<Con: BulkConsumer<Item = u8>>(&self, consumer: &mut Con) -> Result<(), Con::Error>;
}

/// Returns the number of octets needed to store a number, along the lines of
/// 8-bit, 16-bit, 32-bit, or 64-bit unsigned integers.
///
/// https://willowprotocol.org/specs/encodings/index.html#compact_width
pub fn compact_width(value: usize) -> usize {
    if value < 256 {
        1
    } else if value < 65536 {
        2
    } else if value < 2147483648 {
        4
    } else {
        8
    }
}

/// Encode a `usize` integer using the smallest number of bytes possible.
pub fn encode_usize<Con: BulkConsumer<Item = u8>>(
    value: usize,
    consumer: &mut Con,
) -> Result<(), Con::Error> {
    match compact_width(value) {
        1 => {
            let compact_value = value as u8;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        2 => {
            let compact_value = value as u16;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        4 => {
            let compact_value = value as u32;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        8 => {
            let compact_value = value as u64;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}

/// Everything that can go wrong when decoding a value.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DecodeError<ProducerError> {
    Producer(ProducerError),
    GarbageInput,
}

pub trait Decoder: Sized {
    // unimplemented!();
}

fn decode_u8<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<usize, DecodeError<Pro::Error>> {
    // An array of 1 byte is required to decode the length.
    let mut buf: [MaybeUninit<u8>; 1] = MaybeUninit::uninit_array();

    // Fill the buffer with bytes from the producer.
    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    // Convert the slice of bytes to a fixed-size array.
    let byte_array: [u8; 1] = buf_init.try_into().expect("array to be correctly sized");

    // Create an integer value from the bytes and cast to usize.
    let value = u8::from_be_bytes(byte_array) as usize;

    Ok(value)
}

fn decode_u16<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<usize, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 1] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 2] = buf_init.try_into().expect("array to be correctly sized");

    let value = u16::from_be_bytes(byte_array) as usize;

    Ok(value)
}

fn decode_u32<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<usize, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 4] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 4] = buf_init.try_into().expect("array to be correctly sized");

    let value = u32::from_be_bytes(byte_array) as usize;

    Ok(value)
}

fn decode_u64<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<usize, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 8] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 8] = buf_init.try_into().expect("array to be correctly sized");

    let value = u64::from_be_bytes(byte_array) as usize;

    Ok(value)
}

/// Decode the bytes representing a variable width integer into a `usize`.
///
/// The `max` parameter defines the largest possible number which may be
/// decoded.
pub fn decode_usize<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
    max: usize,
) -> Result<usize, DecodeError<Pro::Error>> {
    match compact_width(max) {
        1 => decode_u8(producer),
        2 => decode_u16(producer),
        4 => decode_u32(producer),
        8 => decode_u64(producer),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_correct_compact_width() {
        assert_eq!(compact_width(200), 1);
        assert_eq!(compact_width(7000), 2);
        assert_eq!(compact_width(80000), 4);
        assert_eq!(compact_width(3000000000), 8);
    }
}
