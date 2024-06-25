// Encoders and decoders.

// TODO: Move encoders and decoders into separate modules.

use core::mem::MaybeUninit;

use std::fmt::Debug;

use thiserror::Error;
use ufotofu::sync::{self, BulkConsumer, BulkProducer};

/// Returns the number of octets needed to store a number, along the lines of
/// 8-bit, 16-bit, 32-bit, or 64-bit unsigned integers.
///
/// https://willowprotocol.org/specs/encodings/index.html#compact_width
pub fn compact_width(value: usize) -> u8 {
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

/// Encode a `usize` integer using the smallest possible number of
/// big-endian bytes.
pub fn encode_be_usize<Con: BulkConsumer<Item = u8>>(
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

fn decode_be_u8<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<u8, DecodeError<Pro::Error>> {
    // An array of 1 byte is required to decode the length.
    let mut buf: [MaybeUninit<u8>; 1] = MaybeUninit::uninit_array();

    // Fill the buffer with bytes from the producer.
    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    // Convert the slice of bytes to a fixed-size array.
    let byte_array: [u8; 1] = buf_init.try_into().expect("array to be correctly sized");

    // Create an integer value from the bytes and cast to usize.
    let value = u8::from_be_bytes(byte_array);

    Ok(value)
}

fn decode_be_u16<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<u16, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 2] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 2] = buf_init.try_into().expect("array to be correctly sized");

    let value = u16::from_be_bytes(byte_array);

    Ok(value)
}

fn decode_be_u32<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<u32, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 4] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 4] = buf_init.try_into().expect("array to be correctly sized");

    let value = u32::from_be_bytes(byte_array);

    Ok(value)
}

/// Decode a `u64` value from big-endian bytes produced by the given
/// `Producer`.
fn decode_be_u64<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<u64, DecodeError<Pro::Error>> {
    let mut buf: [MaybeUninit<u8>; 8] = MaybeUninit::uninit_array();

    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    let byte_array: [u8; 8] = buf_init.try_into().expect("array to be correctly sized");

    let value = u64::from_be_bytes(byte_array);

    Ok(value)
}

/// Decode the bytes representing a variable width integer into a `usize`.
///
/// The `compact_width` parameter defines the number of bytes required
/// to store the decoded the value.
pub fn decode_be_usize<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
    compact_width: u8,
) -> Result<usize, DecodeError<Pro::Error>> {
    match compact_width {
        1 => decode_be_u8(producer).map(|val| val as usize),
        2 => decode_be_u16(producer).map(|val| val as usize),
        4 => decode_be_u32(producer).map(|val| val as usize),
        8 => decode_be_u64(producer).map(|val| val as usize),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use ufotofu::sync::producer::SliceProducer;

    use super::*;

    #[test]
    fn returns_correct_compact_width() {
        assert_eq!(compact_width(200), 1);
        assert_eq!(compact_width(7000), 2);
        assert_eq!(compact_width(80000), 4);
        assert_eq!(compact_width(3000000000), 8);
    }

    #[test]
    fn decodes_be_u8() {
        // Define the encoded value (7) followed by additional bytes.
        let encoded_value = [7];
        let encoded_remainder = [119, 104, 105, 115, 112, 101, 114];
        let encoded_bytes = [&encoded_value[..], &encoded_remainder[..]].concat();

        // Create a `Producer` with encoded bytes.
        let mut producer = SliceProducer::new(&encoded_bytes);

        // Decode the `u8` from the `Producer`.
        let decoded_width = decode_be_usize(&mut producer, 1).unwrap();
        assert_eq!(decoded_width, 7);

        // Ensure that the remaining bytes are correct.
        let mut buf: [MaybeUninit<u8>; 8] = MaybeUninit::uninit_array();
        let (remaining_bytes, _buf_maybe_uninit) = sync::fill_all(&mut buf, &mut producer).unwrap();
        assert_eq!(remaining_bytes, encoded_remainder);
    }

    #[test]
    fn decodes_be_u16() {
        let encoded_value = [255, 220];
        let encoded_remainder = [119, 104, 105, 115, 112, 101];
        let encoded_bytes = [&encoded_value[..], &encoded_remainder[..]].concat();

        let mut producer = SliceProducer::new(&encoded_bytes);

        let decoded_width = decode_be_usize(&mut producer, 2).unwrap();
        assert_eq!(decoded_width, 65500);

        let mut buf: [MaybeUninit<u8>; 8] = MaybeUninit::uninit_array();
        let (remaining_bytes, _buf_maybe_uninit) = sync::fill_all(&mut buf, &mut producer).unwrap();
        assert_eq!(remaining_bytes, encoded_remainder);
    }
}
