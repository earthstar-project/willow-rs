use crate::path::PathComponent;
use ufotofu::local_nb::BulkConsumer;
use ufotofu::local_nb::BulkProducer;

use crate::encoding::error::DecodeError;
use crate::path::Path;

use crate::encoding::error::EncodingConsumerError;

/// Return the least natural number such that 256^`n` is greater than or equal to `n`.
///
/// Used for determining the minimal number of bytes needed to represent a given unsigned integer, and more specifically [`path_length_power`](https://willowprotocol.org/specs/encodings/index.html#path_length_power) and [`path_count_power`](https://willowprotocol.org/specs/encodings/index.html#path_count_power).
pub const fn get_max_power(n: usize) -> u8 {
    if n < 256 {
        1
    } else if n < 65536 {
        2
    } else if n < 16777216 {
        3
    } else if n < 4294967296 {
        4
    } else if n < (256_usize).pow(5) {
        5
    } else if n < (256_usize).pow(6) {
        6
    } else if n < (256_usize).pow(7) {
        7
    } else {
        8
    }
}

/// Encode the given [`Path`] ([definition](https://willowprotocol.org/specs/encodings/index.html#enc_path)), and consume the result with a [`ufotofu::local_nb::BulkConsumer`].
pub async fn encode_path<
    const MCL: usize,
    const MCC: usize,
    P: Path,
    Consumer: BulkConsumer<Item = u8>,
>(
    path: &P,
    consumer: &mut Consumer,
) -> Result<(), EncodingConsumerError<Consumer::Error>> {
    let path_length_power = get_max_power(MCL);
    let path_count_power = get_max_power(MCC);

    let path_count_raw: [u8; 8] = path.component_count().to_be_bytes();

    consumer
        .bulk_consume_full_slice(&path_count_raw[8 - (path_count_power as usize)..])
        .await?;

    for component in path.components() {
        let component_length_raw = component.len().to_be_bytes();

        consumer
            .bulk_consume_full_slice(&component_length_raw[8 - (path_length_power as usize)..])
            .await?;

        if component.len() > 0 {
            consumer.bulk_consume_full_slice(component.as_ref()).await?;
        }
    }

    Ok(())
}

pub async fn decode_path<
    const MCL: usize,
    const MCC: usize,
    Producer: BulkProducer<Item = u8>,
    P: Path,
>(
    producer: &mut Producer,
) -> Result<P, DecodeError<Producer::Error>> {
    let mut component_count_slice = [0u8; 8];
    let path_count_power = get_max_power(MCC);
    let path_length_power = get_max_power(MCL);

    producer
        .bulk_overwrite_full_slice(&mut component_count_slice[8 - (path_count_power as usize)..])
        .await?;

    let component_count = u64::from_be_bytes(component_count_slice);

    let mut path = P::empty();

    for _ in 0..component_count {
        let mut component_len_slice = [0u8; 8];

        producer
            .bulk_overwrite_full_slice(&mut component_len_slice[8 - (path_length_power as usize)..])
            .await?;

        let component_len = u64::from_be_bytes(component_len_slice);

        let mut component_box = Box::new_uninit_slice(usize::try_from(component_len)?);

        let slice = producer
            .bulk_overwrite_full_slice_uninit(component_box.as_mut())
            .await?;

        let path_component = P::Component::new(slice).map_err(|_| DecodeError::InvalidInput)?;
        path = path
            .append(path_component)
            .map_err(|_| DecodeError::InvalidInput)?;
    }

    Ok(path)
}
