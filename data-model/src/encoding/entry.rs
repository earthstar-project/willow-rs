use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{DecodingFunction, EncodingFunction},
        path::{decode_path, encode_path},
    },
    entry::Entry,
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

pub async fn encode_entry<const MCL: usize, const MCC: usize, N, NE, S, SE, P, PD, PDE, Consumer>(
    entry: &Entry<N, S, P, PD>,
    encode_namespace_id: NE,
    encode_subspace_id: SE,
    encode_payload_digest: PDE,
    consumer: &mut Consumer,
) -> Result<(), EncodingConsumerError<Consumer::Error>>
where
    N: NamespaceId,
    NE: EncodingFunction<N, Consumer>,
    S: SubspaceId,
    SE: EncodingFunction<S, Consumer>,
    P: Path,
    PD: PayloadDigest,
    PDE: EncodingFunction<PD, Consumer>,
    Consumer: BulkConsumer<Item = u8>,
{
    encode_namespace_id(&entry.namespace_id, consumer).await?;
    encode_subspace_id(&entry.subspace_id, consumer).await?;
    encode_path::<MCL, MCC, _, _>(&entry.path, consumer).await?;

    consumer
        .bulk_consume_full_slice(&entry.timestamp.to_be_bytes())
        .await?;

    consumer
        .bulk_consume_full_slice(&entry.payload_length.to_be_bytes())
        .await?;

    encode_payload_digest(&entry.payload_digest, consumer).await?;

    Ok(())
}

pub async fn decode_entry<const MCL: usize, const MCC: usize, N, ND, S, SD, P, PD, PDD, Producer>(
    decode_namespace_id: ND,
    decode_subspace_id: SD,
    decode_payload_digest: PDD,
    producer: &mut Producer,
) -> Result<Entry<N, S, P, PD>, DecodeError<Producer::Error>>
where
    N: NamespaceId,
    ND: DecodingFunction<N, Producer>,
    S: SubspaceId,
    SD: DecodingFunction<S, Producer>,
    P: Path,
    PD: PayloadDigest,
    PDD: DecodingFunction<PD, Producer>,
    Producer: BulkProducer<Item = u8>,
{
    let namespace_id: N = decode_namespace_id(producer).await?;
    let subspace_id: S = decode_subspace_id(producer).await?;
    let path: P = decode_path::<MCL, MCC, _, _>(producer).await?;

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

    let payload_digest = decode_payload_digest(producer).await?;

    Ok(Entry {
        namespace_id,
        subspace_id,
        path,
        timestamp,
        payload_length,
        payload_digest,
    })
}
