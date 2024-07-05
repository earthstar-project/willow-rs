use crate::encoding::error::DecodeError;
use either::Either;
use ufotofu::local_nb::consumer::pipe_from_slice;
use ufotofu::local_nb::consumer::PipeFromSliceError;
use ufotofu::local_nb::producer::bulk_pipe_into_slice;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

/// Return the most compact width in bytes (1, 2, 4, or 8) needed to represent a given `u64` as a corresponding 8-bit, 16-bit, 32-bit, or 64-bit number.
///
/// [Definition](https://willowprotocol.org/specs/encodings/index.html#compact_width).
pub fn compact_width(value: u64) -> u8 {
    if value <= u8::MAX as u64 {
        1
    } else if value <= u16::MAX as u64 {
        2
    } else if value <= u32::MAX as u64 {
        4
    } else {
        8
    }
}

/// Encode a `u64` integer as a `compact_width(value)`-byte big-endian integer, and consume that with a [`BulkConsumer`].
pub async fn encode_compact_width_be<Consumer: BulkConsumer<Item = u8>>(
    value: u64,
    consumer: &mut Consumer,
) -> Result<(), PipeFromSliceError<Consumer::Error>> {
    match compact_width(value) {
        1 => pipe_from_slice(&(value as u8).to_be_bytes(), consumer).await,
        2 => pipe_from_slice(&(value as u16).to_be_bytes(), consumer).await,
        4 => pipe_from_slice(&(value as u32).to_be_bytes(), consumer).await,
        8 => pipe_from_slice(&value.to_be_bytes(), consumer).await,
        _ => unreachable!("compact_width returned a invalid width"),
    }
}

pub async fn decode_u8_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u8, DecodeError<Either<Producer::Final, Producer::Error>>> {
    let mut byte = [0];

    match bulk_pipe_into_slice(&mut byte, producer).await {
        Ok(_) => Ok(u8::from_be_bytes(byte)),
        Err(err) => Err(DecodeError::Producer(err.reason)),
    }
}

pub async fn decode_u16_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u16, DecodeError<Either<Producer::Final, Producer::Error>>> {
    let mut byte: [u8; 2] = [0, 0];

    match bulk_pipe_into_slice(&mut byte, producer).await {
        Ok(_) => Ok(u16::from_be_bytes(byte)),
        Err(err) => Err(DecodeError::Producer(err.reason)),
    }
}

pub async fn decode_u32_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u32, DecodeError<Either<Producer::Final, Producer::Error>>> {
    let mut byte: [u8; 4] = [0, 0, 0, 0];

    match bulk_pipe_into_slice(&mut byte, producer).await {
        Ok(_) => Ok(u32::from_be_bytes(byte)),
        Err(err) => Err(DecodeError::Producer(err.reason)),
    }
}

pub async fn decode_u64_be<Producer: BulkProducer<Item = u8>>(
    producer: &mut Producer,
) -> Result<u64, DecodeError<Either<Producer::Final, Producer::Error>>> {
    let mut byte: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

    match bulk_pipe_into_slice(&mut byte, producer).await {
        Ok(_) => Ok(u64::from_be_bytes(byte)),
        Err(err) => Err(DecodeError::Producer(err.reason)),
    }
}

/// Decode the bytes representing a [`compact-width`]-bytes integer into a `usize`.
pub async fn decode_compact_width_be<Producer: BulkProducer<Item = u8>>(
    compact_width: u8,
    producer: &mut Producer,
) -> Result<u64, DecodeError<Either<Producer::Final, Producer::Error>>> {
    match compact_width {
        1 => decode_u8_be(producer).await.map(|v| v as u64),
        2 => decode_u16_be(producer).await.map(|v| v as u64),
        4 => decode_u32_be(producer).await.map(|v| v as u64),
        8 => decode_u64_be(producer).await,
        _ => panic!("decode_compact_width provided an invalid u8 as compact width!"),
    }
}

#[cfg(test)]
mod tests {
    use ufotofu::local_nb::consumer::IntoVec;
    use ufotofu::local_nb::producer::FromVec;

    use super::*;

    #[test]
    fn compact_width_works() {
        assert_eq!(compact_width(0), 1);
        assert_eq!(compact_width(u8::MAX as u64), 1);

        assert_eq!(compact_width(u8::MAX as u64 + 1), 2);
        assert_eq!(compact_width(u16::MAX as u64), 2);

        assert_eq!(compact_width(u16::MAX as u64 + 1), 4);
        assert_eq!(compact_width(u32::MAX as u64), 4);

        assert_eq!(compact_width(u32::MAX as u64 + 1), 8);
        assert_eq!(compact_width(u64::MAX), 8);
    }

    #[test]
    fn encoding() {
        let values = [
            (1, 0),
            (1, u8::MAX as u64),
            (2, u8::MAX as u64 + 1),
            (2, u16::MAX as u64),
            (4, u16::MAX as u64 + 1),
            (4, u32::MAX as u64),
            (8, u32::MAX as u64 + 1),
            (8, u64::MAX),
        ];

        smol::block_on(async {
            for (width, value) in values {
                let mut consumer = IntoVec::<u8>::new();

                encode_compact_width_be(value, &mut consumer).await.unwrap();

                let encode_result = consumer.into_vec();

                assert_eq!(encode_result.len(), width);

                let mut producer = FromVec::new(encode_result);

                let decode_result = decode_compact_width_be(width as u8, &mut producer)
                    .await
                    .unwrap();

                assert_eq!(decode_result, value);
            }
        });
    }
}
