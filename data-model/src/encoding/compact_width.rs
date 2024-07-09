use crate::encoding::error::DecodeError;
use ufotofu::common::errors::ConsumeFullSliceError;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// A minimum width of bytes needed to represent a unsigned integer.
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
pub struct NotACompactWidthError(pub u8);

impl CompactWidth {
    /// Return a new [`CompactWidth`]
    pub fn new(n: u8) -> Result<CompactWidth, NotACompactWidthError> {
        match n {
            1 => Ok(CompactWidth::One),
            2 => Ok(CompactWidth::Two),
            4 => Ok(CompactWidth::Four),
            8 => Ok(CompactWidth::Eight),
            _ => Err(NotACompactWidthError(n)),
        }
    }
}

/// Return the most compact width in bytes (1, 2, 4, or 8) needed to represent a given `u64` as a corresponding 8-bit, 16-bit, 32-bit, or 64-bit number.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
// Doc tests?!
pub fn compact_width(value: u64) -> CompactWidth {
    // There is a branchless implementation of this via logarithms, but that's for later and requires benchmarking.
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

/// Encode a `u64` integer as a `compact_width(value)`-byte big-endian integer, and consume that with a [`BulkConsumer`].
pub async fn encode_compact_width_be<Consumer: BulkConsumer<Item = u8>>(
    value: u64,
    consumer: &mut Consumer,
) -> Result<(), ConsumeFullSliceError<Consumer::Error>> {
    let width = match compact_width(value) {
        CompactWidth::One => 1,
        CompactWidth::Two => 2,
        CompactWidth::Four => 4,
        CompactWidth::Eight => 8,
    };

    consumer
        .bulk_consume_full_slice(&value.to_be_bytes()[8 - width..])
        .await
}

pub async fn decode_u8_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u8, DecodeError<Producer::Error>> {
    let mut byte = [0; 1];
    producer.bulk_overwrite_full_slice(&mut byte).await?;
    Ok(u8::from_be_bytes(byte))
}

pub async fn decode_u16_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u16, DecodeError<Producer::Error>> {
    let mut bytes = [0u8; 2];
    producer.bulk_overwrite_full_slice(&mut bytes).await?;
    Ok(u16::from_be_bytes(bytes))
}

pub async fn decode_u32_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u32, DecodeError<Producer::Error>> {
    let mut bytes = [0u8; 4];
    producer.bulk_overwrite_full_slice(&mut bytes).await?;
    Ok(u32::from_be_bytes(bytes))
}

pub async fn decode_u64_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u64, DecodeError<Producer::Error>> {
    let mut bytes = [0u8; 8];
    producer.bulk_overwrite_full_slice(&mut bytes).await?;
    Ok(u64::from_be_bytes(bytes))
}

/// Decode the bytes representing a [`compact-width`]-bytes integer into a `usize`.
pub async fn decode_compact_width_be<Producer: BulkProducer<Item = u8>>(
    compact_width: CompactWidth,
    producer: &mut Producer,
) -> Result<u64, DecodeError<Producer::Error>> {
    match compact_width {
        CompactWidth::One => decode_u8_be(producer).await.map(|v| v as u64),
        CompactWidth::Two => decode_u16_be(producer).await.map(|v| v as u64),
        CompactWidth::Four => decode_u32_be(producer).await.map(|v| v as u64),
        CompactWidth::Eight => decode_u64_be(producer).await,
    }
}

#[cfg(test)]
mod tests {
    use ufotofu::local_nb::consumer::IntoVec;
    use ufotofu::local_nb::producer::FromVec;

    use super::*;

    #[test]
    fn compact_width_works() {
        assert_eq!(compact_width(0), CompactWidth::One);
        assert_eq!(compact_width(u8::MAX as u64), CompactWidth::One);

        assert_eq!(compact_width(u8::MAX as u64 + 1), CompactWidth::Two);
        assert_eq!(compact_width(u16::MAX as u64), CompactWidth::Two);

        assert_eq!(compact_width(u16::MAX as u64 + 1), CompactWidth::Four);
        assert_eq!(compact_width(u32::MAX as u64), CompactWidth::Four);

        assert_eq!(compact_width(u32::MAX as u64 + 1), CompactWidth::Eight);
        assert_eq!(compact_width(u64::MAX), CompactWidth::Eight);
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

        smol::block_on(async {
            for (width, value) in values {
                let mut consumer = IntoVec::<u8>::new();

                encode_compact_width_be(value, &mut consumer).await.unwrap();

                let encode_result = consumer.into_vec();

                let decoded_compact_width = CompactWidth::new(encode_result.len() as u8).unwrap();

                assert_eq!(decoded_compact_width, width);

                let mut producer = FromVec::new(encode_result);

                let decode_result = decode_compact_width_be(decoded_compact_width, &mut producer)
                    .await
                    .unwrap();

                assert_eq!(decode_result, value);
            }
        });
    }
}