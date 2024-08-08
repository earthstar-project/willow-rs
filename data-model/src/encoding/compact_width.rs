use crate::encoding::error::DecodeError;

/// A minimum width of bytes needed to represent a unsigned integer.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width)
#[derive(PartialEq, Eq, Debug)]
pub enum CompactWidth {
    /// The byte-width required to represent numbers up to 256 (i.e. a 8-bit number).
    One,
    /// The byte-width required to represent numbers up to 256^2 (i.e. a 16-bit number).
    Two,
    /// The byte-width required to represent numbers up to 256^4 (i.e. a 32-bit number).
    Four,
    /// The byte-width required to represent numbers up to 256^8 (i.e. a 64-bit number).
    Eight,
}

#[derive(Debug)]
pub(crate) struct NotACompactWidthError();

impl CompactWidth {
    /// Return a new [`CompactWidth`].
    pub(crate) fn new(n: u8) -> Result<CompactWidth, NotACompactWidthError> {
        match n {
            1 => Ok(CompactWidth::One),
            2 => Ok(CompactWidth::Two),
            4 => Ok(CompactWidth::Four),
            8 => Ok(CompactWidth::Eight),
            _ => Err(NotACompactWidthError()),
        }
    }

    /// Return the most compact width in bytes (1, 2, 4, or 8) needed to represent a given `u64` as a corresponding 8-bit, 16-bit, 32-bit, or 64-bit number.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
    pub fn from_u64(value: u64) -> Self {
        if value <= u8::MAX as u64 {
            CompactWidth::One
        } else if value <= u16::MAX as u64 {
            CompactWidth::Two
        } else if value <= u32::MAX as u64 {
            CompactWidth::Four
        } else {
            CompactWidth::Eight
        }
    }

    /// Return the most compact width in bytes (1, 2, 4) needed to represent a given `u32` as a corresponding 8-bit, 16-bit, or 32-bit number.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
    pub fn from_u32(value: u32) -> Self {
        if value <= u8::MAX as u32 {
            CompactWidth::One
        } else if value <= u16::MAX as u32 {
            CompactWidth::Two
        } else {
            CompactWidth::Four
        }
    }

    /// Return the most compact width in bytes (1 or 2) needed to represent a given `u16` as a corresponding 8-bit or 16-bit number.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
    pub fn from_u16(value: u16) -> Self {
        if value <= u8::MAX as u16 {
            CompactWidth::One
        } else {
            CompactWidth::Two
        }
    }

    /// Return [`CompactWidth::One`], the only [`CompactWidth`] needed to represent a given `u8`.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
    pub fn from_u8(_: u8) -> Self {
        CompactWidth::One
    }

    /// Return the width in bytes of this [`CompactSize`].
    pub fn width(&self) -> usize {
        match self {
            CompactWidth::One => 1,
            CompactWidth::Two => 2,
            CompactWidth::Four => 4,
            CompactWidth::Eight => 8,
        }
    }

    /// Encode a [`CompactWidth`] as a 2-bit integer `n` such that 2^n gives the bytewidth of the [`CompactWidth`], and then place that 2-bit number into a `u8` at the bit-index of `position`.
    pub fn bitmask(&self, position: u8) -> u8 {
        let og = match self {
            CompactWidth::One => 0b0000_0000,
            CompactWidth::Two => 0b0100_0000,
            CompactWidth::Four => 0b1000_0000,
            CompactWidth::Eight => 0b1100_0000,
        };

        og >> position
    }

    pub fn decode_fixed_width_bitmask(mask: u8, offset: u8) -> Self {
        let twobit_mask = 0b0000_0011;
        let two_bit_int = mask >> (6 - offset) & twobit_mask;

        // Because we sanitise the input down to a 2-bit integer, we can safely unwrap this.
        CompactWidth::new(2u8.pow(two_bit_int as u32)).unwrap()
    }
}

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
pub mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    use crate::encoding::unsigned_int::{U16BE, U32BE, U64BE, U8BE};

    #[syncify_replace(use crate::encoding::sync::{Decodable};)]
    use crate::encoding::Decodable;

    /// Encode a `u64` integer as a `compact_width(value)`-byte big-endian integer, and consume that with a [`BulkConsumer`].
    pub async fn encode_compact_width_be<Consumer: BulkConsumer<Item = u8>>(
        value: u64,
        consumer: &mut Consumer,
    ) -> Result<(), Consumer::Error> {
        let width = CompactWidth::from_u64(value).width();

        consumer
            .bulk_consume_full_slice(&value.to_be_bytes()[8 - width..])
            .await
            .map_err(|f| f.reason)?;

        Ok(())
    }

    /// Decode the bytes representing a [`compact-width`]-bytes integer into a `usize`.
    pub async fn decode_compact_width_be<Producer: BulkProducer<Item = u8>>(
        compact_width: CompactWidth,
        producer: &mut Producer,
    ) -> Result<u64, DecodeError<Producer::Error>> {
        let decoded = match compact_width {
            CompactWidth::One => U8BE::decode(producer).await.map(u64::from),
            CompactWidth::Two => U16BE::decode(producer).await.map(u64::from),
            CompactWidth::Four => U32BE::decode(producer).await.map(u64::from),
            CompactWidth::Eight => U64BE::decode(producer).await.map(u64::from),
        }?;

        let real_width = CompactWidth::from_u64(decoded);

        if real_width != compact_width {
            return Err(DecodeError::InvalidInput);
        }

        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use encoding_sync::{decode_compact_width_be, encode_compact_width_be};
    use ufotofu::local_nb::consumer::IntoVec;
    use ufotofu::local_nb::producer::FromBoxedSlice;

    use super::*;

    #[test]
    fn compact_width_works() {
        // u64
        assert_eq!(CompactWidth::from_u64(0_u64), CompactWidth::One);
        assert_eq!(CompactWidth::from_u64(u8::MAX as u64), CompactWidth::One);

        assert_eq!(
            CompactWidth::from_u64(u8::MAX as u64 + 1),
            CompactWidth::Two
        );
        assert_eq!(CompactWidth::from_u64(u16::MAX as u64), CompactWidth::Two);

        assert_eq!(
            CompactWidth::from_u64(u16::MAX as u64 + 1),
            CompactWidth::Four
        );
        assert_eq!(CompactWidth::from_u64(u32::MAX as u64), CompactWidth::Four);

        assert_eq!(
            CompactWidth::from_u64(u32::MAX as u64 + 1),
            CompactWidth::Eight
        );
        assert_eq!(CompactWidth::from_u64(u64::MAX), CompactWidth::Eight);

        // u32
        assert_eq!(CompactWidth::from_u32(0_u32), CompactWidth::One);
        assert_eq!(CompactWidth::from_u32(u8::MAX as u32), CompactWidth::One);

        assert_eq!(
            CompactWidth::from_u32(u8::MAX as u32 + 1),
            CompactWidth::Two
        );
        assert_eq!(CompactWidth::from_u32(u16::MAX as u32), CompactWidth::Two);

        assert_eq!(
            CompactWidth::from_u32(u16::MAX as u32 + 1),
            CompactWidth::Four
        );
        assert_eq!(CompactWidth::from_u32(u32::MAX), CompactWidth::Four);

        // u16
        assert_eq!(CompactWidth::from_u16(0_u16), CompactWidth::One);
        assert_eq!(CompactWidth::from_u16(u8::MAX as u16), CompactWidth::One);

        assert_eq!(
            CompactWidth::from_u16(u8::MAX as u16 + 1),
            CompactWidth::Two
        );
        assert_eq!(CompactWidth::from_u16(u16::MAX), CompactWidth::Two);

        // u8
        assert_eq!(CompactWidth::from_u8(0_u8), CompactWidth::One);
        assert_eq!(CompactWidth::from_u8(u8::MAX), CompactWidth::One);
    }

    #[test]
    fn encoding() {
        let values = [
            (CompactWidth::One, 0),
            (CompactWidth::One, u8::MAX as u64),
            (CompactWidth::Two, u8::MAX as u64 + 1),
            (CompactWidth::Two, u16::MAX as u64),
            (CompactWidth::Four, u16::MAX as u64 + 1),
            (CompactWidth::Four, u32::MAX as u64),
            (CompactWidth::Eight, u32::MAX as u64 + 1),
            (CompactWidth::Eight, u64::MAX),
        ];

        for (compact_width, value) in values {
            let mut consumer = IntoVec::<u8>::new();

            encode_compact_width_be(value, &mut consumer).unwrap();

            let encode_result = consumer.into_vec();

            let decoded_compact_width = CompactWidth::new(encode_result.len() as u8).unwrap();

            assert_eq!(decoded_compact_width, compact_width);

            let mut producer = FromBoxedSlice::from_vec(encode_result);

            let decode_result =
                decode_compact_width_be(decoded_compact_width, &mut producer).unwrap();

            assert_eq!(decode_result, value);
        }
    }
}
