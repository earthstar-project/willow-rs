use std::convert::Infallible;

use compact_u64::CompactU64;
use either::Either;
use ufotofu::{bulk_pipe, BulkConsumer, BulkProducer, Producer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeEncodable,
};
use willow_data_model::{
    grouping::{Area, AreaOfInterest},
    AuthorisationToken, AuthorisedEntry, Entry, NamespaceId, Path, PayloadDigest,
    QueryIgnoreParams, QueryOrder, Store, SubspaceId, UnauthorisedWriteError,
};

pub trait SideloadNamespaceId:
    NamespaceId + EncodableSync + EncodableKnownSize + Decodable + DecodableCanonic
{
    /// The least element of the set of namespace IDs.
    const DEFAULT_NAMESPACE_ID: Self;
}

pub trait SideloadSubspaceId:
    SubspaceId + EncodableSync + EncodableKnownSize + Decodable + DecodableCanonic
{
    /// The least element of the set of subspace IDs.
    const DEFAULT_SUBSPACE_ID: Self;
}

pub trait SideloadPayloadDigest:
    PayloadDigest + EncodableSync + EncodableKnownSize + Decodable + DecodableCanonic
{
    /// The least element of the set of payload digests.
    const DEFAULT_PAYLOAD_DIGEST: Self;
}

pub trait SideloadAuthorisationToken<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
>:
    AuthorisationToken<MCL, MCC, MPL, N, S, PD>
    + EncodableSync
    + EncodableKnownSize
    + Decodable
    + DecodableCanonic
{
}

pub enum CreateDropError<ConsumerError> {
    EmptyDrop,
    StoreErr,
    ConsumerProblem(ConsumerError),
}

pub enum IngestDropError<ProducerError> {
    StoreErr,
    UnexpectedEndOfInput,
    ProducerProblem(ProducerError),
    UnauthorisedEntry(UnauthorisedWriteError),
    Other,
}

impl<E> From<UnauthorisedWriteError> for IngestDropError<E> {
    fn from(value: UnauthorisedWriteError) -> Self {
        IngestDropError::UnauthorisedEntry(value)
    }
}

impl<F, E, O> From<DecodeError<F, E, O>> for IngestDropError<E> {
    fn from(value: DecodeError<F, E, O>) -> Self {
        match value {
            DecodeError::UnexpectedEndOfInput(_) => IngestDropError::UnexpectedEndOfInput,
            DecodeError::Producer(err) => IngestDropError::ProducerProblem(err),
            DecodeError::Other(_) => IngestDropError::Other,
        }
    }
}

pub async fn create_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: SideloadAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    C,
    EncryptedC,
    EncryptFn,
    StoreType,
    AreaIterator,
>(
    consumer: C,
    encrypt: EncryptFn,
    namespace_id: N,
    areas: AreaIterator,
    store: &StoreType,
) -> Result<(), CreateDropError<EncryptedC::Error>>
where
    C: BulkConsumer<Item = u8>,
    EncryptedC: BulkConsumer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    AreaIterator: IntoIterator<Item = Area<MCL, MCC, MPL, S>>,
    EncryptFn: Fn(C) -> EncryptedC,
{
    // https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol

    let mut encrypted_consumer = encrypt(consumer);

    let mut entries_count = 0;

    // TODO: Order / maybe even merge given areas to create the highest probability of efficiently relatively encoded entries.

    // Doing this so we can loop over areas twice.
    let mut next_areas_vec: Vec<Area<MCL, MCC, MPL, S>> = Vec::new();

    // Get the total number of included entries.
    for area in areas {
        entries_count += store.count_area(&area.clone().into()).await;
        next_areas_vec.push(area);
    }

    // And encode them into the drop.
    CompactU64(entries_count)
        .encode(&mut encrypted_consumer)
        .await
        .map_err(CreateDropError::ConsumerProblem)?;

    namespace_id
        .encode(&mut encrypted_consumer)
        .await
        .map_err(CreateDropError::ConsumerProblem)?;

    // Set the initial entry to relatively encode against
    let mut entry_to_encode_against = Entry::new(
        namespace_id,
        S::default(),
        Path::<MCL, MCC, MPL>::new_empty(),
        0,
        0,
        PD::default(),
    );

    // For each area,
    for area in next_areas_vec {
        let aoi: AreaOfInterest<MCL, MCC, MPL, S> = area.into();

        // Get the producer of entries from this area
        let mut entry_producer = store.query_area(
            &aoi,
            &QueryOrder::Subspace,
            false,
            Some(QueryIgnoreParams {
                ignore_incomplete_payloads: true,
                ignore_empty_payloads: true,
            }),
        );

        loop {
            match entry_producer.produce().await {
                Ok(Either::Left(lengthy)) => {
                    let authed_entry = lengthy.entry();

                    // Encode entry relative to previously encoded entry
                    let entry = authed_entry.entry();
                    entry
                        .relative_encode(&mut encrypted_consumer, &entry_to_encode_against)
                        .await
                        .map_err(CreateDropError::ConsumerProblem)?;

                    entry_to_encode_against = entry.clone();

                    // Encode token
                    let token = authed_entry.token();
                    token
                        .encode(&mut encrypted_consumer)
                        .await
                        .map_err(CreateDropError::ConsumerProblem)?;

                    // Consume payload
                    let mut payload = store
                        .payload(entry.payload_digest())
                        .await
                        .ok_or(CreateDropError::StoreErr)?;

                    bulk_pipe(&mut payload, &mut encrypted_consumer).await?;
                }
                Ok(Either::Right(_)) => {
                    break;
                }
                Err(_) => return Err(CreateDropError::StoreErr),
            };
        }
    }

    Ok(())
}

pub async fn ingest_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: SideloadAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    P,
    DecryptedP,
    DecryptFn,
    StoreType,
    AreaIterator,
>(
    producer: P,
    decrypt: DecryptFn,
    store: &StoreType,
) -> Result<(), IngestDropError<DecryptedP::Error>>
where
    P: Producer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    DecryptFn: Fn(P) -> DecryptedP,
    DecryptedP: BulkProducer<Item = u8>,
    Blame: From<N::ErrorReason>
        + From<S::ErrorReason>
        + From<PD::ErrorReason>
        + From<N::ErrorCanonic>
        + From<S::ErrorCanonic>
        + From<PD::ErrorCanonic>,
{
    // do the inverse of https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol

    let mut entry_to_decode_against = Entry::new(
        N::default(),
        S::default(),
        Path::<MCL, MCC, MPL>::new_empty(),
        0,
        0,
        PD::default(),
    );

    let mut decrypted_producer = decrypt(producer);

    let entries_count = U64BE::decode(&mut decrypted_producer).await?;

    // For each entry we expect,
    for _ in 0..entries_count.0 {
        // Decode entry and token
        let entry =
            Entry::relative_decode(&mut decrypted_producer, &entry_to_decode_against).await?;
        let token = AT::decode(&mut decrypted_producer).await?;

        entry_to_decode_against = entry.clone();

        // Fail if the entry isn't authorised.
        let authed_entry = AuthorisedEntry::new(entry, token)?;

        // Ingest entry
        store
            .ingest_entry(authed_entry, false)
            .await
            .map_err(|_| IngestDropError::StoreErr)?;

        // Ingest payload
        store
            .append_payload(
                entry_to_decode_against.payload_digest(),
                entry_to_decode_against.payload_length(),
                &mut decrypted_producer,
            )
            .await
            .map_err(|_| IngestDropError::StoreErr)?;
    }

    Ok(())
}
