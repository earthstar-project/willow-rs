use std::convert::Infallible;
use std::future::Future;

use compact_u64::{CompactU64, Tag, TagWidth};
use either::Either;
use ufotofu::{bulk_pipe, BulkConsumer, BulkProducer, Producer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeEncodable,
};
use willow_data_model::{
    grouping::Area, AuthorisationToken, AuthorisedEntry, Entry, NamespaceId, Path, PayloadDigest,
    QueryIgnoreParams, Store, SubspaceId, UnauthorisedWriteError,
};

pub trait SideloadNamespaceId: NamespaceId + Encodable + Decodable {
    /// The least element of the set of namespace IDs.
    fn default_namespace_id() -> Self;
}

pub trait SideloadSubspaceId: SubspaceId + EncodableSync + EncodableKnownSize + Decodable {
    /// The least element of the set of subspace IDs.
    fn default_subspace_id() -> Self;
}

pub trait SideloadPayloadDigest: PayloadDigest + Encodable + Decodable {
    /// The least element of the set of payload digests.
    fn default_payload_digest() -> Self;
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
    + Decodable
    + Default
    + RelativeEncodable<(
        AuthorisedEntry<MCL, MCC, MPL, N, S, PD, Self>,
        Entry<MCL, MCC, MPL, N, S, PD>,
    )>
{
}

pub trait SideloadStore<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
>: Store<MCL, MCC, MPL, N, S, PD, AT>
{
    fn count_area(&self, area: &Area<MCL, MCC, MPL, S>) -> impl Future<Output = usize>;
}

#[derive(Debug)]
pub enum CreateDropError<ConsumerError> {
    EmptyDrop,
    StoreErr,
    ConsumerProblem(ConsumerError),
}

#[derive(Debug)]
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
    EncryptedC: BulkConsumer<Item = u8, Final = ()>,
    StoreType: SideloadStore<MCL, MCC, MPL, N, S, PD, AT>,
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
        entries_count += store.count_area(&area).await;
        next_areas_vec.push(area);
    }

    // And encode them into the drop.
    CompactU64(entries_count as u64)
        .encode(&mut encrypted_consumer)
        .await
        .map_err(CreateDropError::ConsumerProblem)?;

    // Encode the namespace ID.
    namespace_id
        .encode(&mut encrypted_consumer)
        .await
        .map_err(CreateDropError::ConsumerProblem)?;

    let entry_minus_one = Entry::new(
        N::default(),
        S::default(),
        Path::<MCL, MCC, MPL>::new_empty(),
        0,
        0,
        PD::default(),
    );

    let auth_minus_one = AT::default();

    let mut prev_authed_entry = AuthorisedEntry::new(entry_minus_one, auth_minus_one).unwrap();

    // For each area
    for area in next_areas_vec {
        // Get the producer of entries from this area.
        let mut entry_producer = store
            .query_area(
                &area,
                QueryIgnoreParams {
                    ignore_incomplete_payloads: true,
                    ignore_empty_payloads: true,
                },
            )
            .await
            .map_err(|_err| CreateDropError::StoreErr)?;

        while let Ok(lengthy) = entry_producer.produce_item().await {
            let authed_entry = lengthy.entry();

            let has_full_payload = lengthy.available() == authed_entry.entry().payload_length();
            let has_different_subspace =
                authed_entry.entry().subspace_id() != prev_authed_entry.entry().subspace_id();
            let timestamp_diff = authed_entry
                .entry()
                .timestamp()
                .abs_diff(prev_authed_entry.entry().timestamp());
            // Cloning this here as we'll need it later - after prev_auth_entry has been moved.
            let prev_path = prev_authed_entry.entry().path().clone();

            // Encode auth token relative to previous authed entry and current entry.
            authed_entry
                .token()
                .relative_encode(
                    &mut encrypted_consumer,
                    &(prev_authed_entry, authed_entry.entry().clone()),
                )
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            // And then a header.
            let mut header = 0;

            if has_full_payload {
                header |= 0b1000_0000;
            }

            if has_different_subspace {
                header |= 0b0100_0000;
            }

            let timestamp_diff_tag = Tag::min_tag(timestamp_diff, TagWidth::two());

            header |= timestamp_diff_tag.data_at_offset(4);

            let payload_length_tag =
                Tag::min_tag(authed_entry.entry().payload_length(), TagWidth::two());

            header |= payload_length_tag.data_at_offset(6);

            encrypted_consumer
                .consume(header)
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            // Encode the subspace if it's different.
            if has_different_subspace {
                authed_entry
                    .entry()
                    .subspace_id()
                    .encode(&mut encrypted_consumer)
                    .await
                    .map_err(CreateDropError::ConsumerProblem)?;
            }

            // Encode the path relative to the last path.
            authed_entry
                .entry()
                .path()
                .relative_encode(&mut encrypted_consumer, &prev_path)
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            // Encode the diff of the timestamps
            CompactU64(timestamp_diff)
                .relative_encode(
                    &mut encrypted_consumer,
                    &timestamp_diff_tag.encoding_width(),
                )
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            // Encode the payload length.
            CompactU64(authed_entry.entry().payload_length())
                .relative_encode(
                    &mut encrypted_consumer,
                    &payload_length_tag.encoding_width(),
                )
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            // Encode the payload digest
            authed_entry
                .entry()
                .payload_digest()
                .encode(&mut encrypted_consumer)
                .await
                .map_err(CreateDropError::ConsumerProblem)?;

            if has_full_payload {
                let payload = store
                    .payload(
                        authed_entry.entry().subspace_id(),
                        authed_entry.entry().path(),
                        0,
                        Some(authed_entry.entry().payload_digest().clone()),
                    )
                    .await
                    .map_err(|_err| CreateDropError::StoreErr)?;

                if let willow_data_model::Payload::Complete(mut payload) = payload {
                    bulk_pipe(&mut payload, &mut encrypted_consumer)
                        .await
                        .map_err(|_err| CreateDropError::StoreErr)?;
                } else {
                    // We apparently had the full payload according to the lengthy.available, but the store says it is incomplete! Problems! no-one will be able to decode this!
                    panic!("The store said it had all the bytes for a given payload, but this turned out not to be true!")
                }
            }

            prev_authed_entry = authed_entry.clone();
        }
    }

    /*
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
        // Get the producer of entries from this area
        let mut entry_producer = store
            .query_area(
                &area,
                QueryIgnoreParams {
                    ignore_incomplete_payloads: true,
                    ignore_empty_payloads: true,
                },
            )
            .await
            .map_err(|_err| CreateDropError::StoreErr)?;

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
                        .payload(
                            entry.subspace_id(),
                            entry.path(),
                            0,
                            Some(entry.payload_digest().clone()),
                        )
                        .await
                        .map_err(|_err| CreateDropError::StoreErr)?;

                    match payload {
                        willow_data_model::Payload::Complete(payload) => {
                            let well = bulk_pipe(&mut payload, &mut encrypted_consumer).await;
                        }
                        willow_data_model::Payload::Incomplete(payload) => {
                            // If we have no bytes, we need a none marker.
                        }
                    }
                }
                Ok(Either::Right(_)) => {
                    break;
                }
                Err(_) => return Err(CreateDropError::StoreErr),
            };
        }
    }

    Ok(())
    */

    todo!()
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
    Blame: From<N::ErrorReason> + From<S::ErrorReason> + From<PD::ErrorReason>,
{
    // do the inverse of https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol

    /*
    let mut entry_to_decode_against = Entry::new(
        N::default(),
        S::default(),
        Path::<MCL, MCC, MPL>::new_empty(),
        0,
        0,
        PD::default(),
    );

    let mut decrypted_producer = decrypt(producer);

    let entries_count = CompactU64::decode(&mut decrypted_producer).await?;

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
    */

    todo!()
}
