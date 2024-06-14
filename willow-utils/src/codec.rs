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
pub fn compact_width(value: &usize) -> usize {
    if *value < 256 {
        1
    } else if *value < 65536 {
        2
    } else if *value < 2147483648 {
        4
    } else {
        8
    }
}

pub fn encode_compact_width<Con: BulkConsumer<Item = u8>>(
    value: &usize,
    consumer: &mut Con,
) -> Result<(), Con::Error> {
    match compact_width(value) {
        1 => {
            let compact_value = *value as u8;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        2 => {
            let compact_value = *value as u16;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        4 => {
            let compact_value = *value as u32;
            sync::consume_all(&compact_value.to_be_bytes(), consumer)?;
        }
        8 => {
            let compact_value = *value as u64;
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

pub fn decode_compact_width<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<usize, DecodeError<Pro::Error>> {
    // The encoded value will be 8 bytes at most.
    let mut buf: [MaybeUninit<u8>; 8] = MaybeUninit::uninit_array();

    // Fill the buffer with bytes from the producer.
    let (buf_init, _buf_maybe_uninit) =
        sync::fill_all(&mut buf, producer).map_err(DecodeError::Producer)?;

    // Convert the slice of bytes to a fixed-size array.
    // Then create an integer value from the bytes and cast to usize.
    let value = match buf_init.len() {
        1 => {
            let byte_array: [u8; 1] = buf_init.try_into().expect("array to be correctly sized");
            u8::from_be_bytes(byte_array) as usize
        }
        2 => {
            let byte_array: [u8; 2] = buf_init.try_into().expect("array to be correctly sized");
            u16::from_be_bytes(byte_array) as usize
        }
        4 => {
            let byte_array: [u8; 4] = buf_init.try_into().expect("array to be correctly sized");
            u32::from_be_bytes(byte_array) as usize
        }
        8 => {
            let byte_array: [u8; 8] = buf_init.try_into().expect("array to be correctly sized");
            u64::from_be_bytes(byte_array) as usize
        }
        _ => unreachable!(),
    };

    Ok(value)
}

pub fn decode_usize<Pro: BulkProducer<Item = u8>>(
    producer: &mut Pro,
) -> Result<u8, DecodeError<Pro::Error>> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_correct_compact_width() {
        assert_eq!(compact_width(&200), 1);
        assert_eq!(compact_width(&7000), 2);
        assert_eq!(compact_width(&80000), 4);
        assert_eq!(compact_width(&3000000000), 8);
    }
}

/*
// iroh/iroh-willow/src/util/codec.rs

//! Traits for encoding and decoding values to and from bytes.

use std::{fmt, io};

/// Trait for encoding values into bytes.
pub trait Encoder: fmt::Debug {
    /// Returns the length (in bytes) of the encoded value.
    fn encoded_len(&self) -> usize;

    /// Encode [`Self`] into a writable buffer which implements `io::Write`.
    fn encode_into<W: io::Write>(&self, out: &mut W) -> anyhow::Result<()>;

    /// Encode [`Self`] into a vector of bytes.
    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.encoded_len());
        self.encode_into(&mut out)?;
        Ok(out)
    }
}

/// Trait for decoding values from bytes.
pub trait Decoder: Sized {
    /// Decode [`Self`] from a byte slice.
    fn decode_from(data: &[u8]) -> anyhow::Result<DecodeOutcome<Self>>;
}

/// The outcome of [`Decoder::decode_from`]
#[derive(Debug)]
pub enum DecodeOutcome<T> {
    /// Not enough data to decode the value.
    NeedMoreData,
    /// Decoded a value.
    Decoded {
        /// The decoded value.
        item: T,
        /// The number of bytes used for decoding the value.
        consumed: usize,
    },
}
*/
