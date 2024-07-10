use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decoder, Encoder},
    },
    entry::Entry,
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

pub async fn encode_entry<N, S, P, PD, Consumer>(
    entry: &Entry<N, S, P, PD>,
    consumer: &mut Consumer,
) -> Result<(), EncodingConsumerError<Consumer::Error>>
where
    N: NamespaceId + Encoder<Consumer>,
    S: SubspaceId + Encoder<Consumer>,
    P: Path + Encoder<Consumer>,
    PD: PayloadDigest + Encoder<Consumer>,
    Consumer: BulkConsumer<Item = u8>,
{
    entry.namespace_id.encode(consumer).await?;
    entry.subspace_id.encode(consumer).await?;
    entry.path.encode(consumer).await?;

    consumer
        .bulk_consume_full_slice(&entry.timestamp.to_be_bytes())
        .await?;

    consumer
        .bulk_consume_full_slice(&entry.payload_length.to_be_bytes())
        .await?;

    entry.payload_digest.encode(consumer).await?;

    Ok(())
}

pub async fn decode_entry<N, S, P, PD, Producer>(
    producer: &mut Producer,
) -> Result<Entry<N, S, P, PD>, DecodeError<Producer::Error>>
where
    N: NamespaceId + Decoder<Producer>,
    S: SubspaceId + Decoder<Producer>,
    P: Path + Decoder<Producer>,
    PD: PayloadDigest + Decoder<Producer>,
    Producer: BulkProducer<Item = u8>,
{
    let namespace_id = N::decode(producer).await?;
    let subspace_id = S::decode(producer).await?;
    let path = P::decode(producer).await?;

    let mut timestamp_bytes = [0u8; 8];
    producer
        .bulk_overwrite_full_slice(&mut timestamp_bytes)
        .await?;
    let timestamp = u64::from_be_bytes(timestamp_bytes);

    let mut payload_length_bytes = [0u8; 8];
    producer
        .bulk_overwrite_full_slice(&mut payload_length_bytes)
        .await?;
    let payload_length = u64::from_be_bytes(payload_length_bytes);

    let payload_digest = PD::decode(producer).await?;

    Ok(Entry {
        namespace_id,
        subspace_id,
        path,
        timestamp,
        payload_length,
        payload_digest,
    })
}
