use compact_u64::{CompactU64, Tag, TagWidth};
use either::Either;
use std::future::Future;
use ufotofu::{bulk_pipe, BulkConsumer, BulkProducer, Producer};
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable,
};
use willow_data_model::{
    grouping::Area, AuthorisationToken, AuthorisedEntry, Entry, EntryIngestionError, NamespaceId,
    Path, PayloadAppendError, PayloadDigest, QueryIgnoreParams, Store, SubspaceId,
    UnauthorisedWriteError,
};
use willow_encoding::is_bitflagged;

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
    )> + RelativeDecodable<
        (
            AuthorisedEntry<MCL, MCC, MPL, N, S, PD, Self>,
            Entry<MCL, MCC, MPL, N, S, PD>,
        ),
        Blame,
    >
{
}

pub trait SideloadStore<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
>: Store<MCL, MCC, MPL, N, S, PD, AT>
{
    fn count_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
    ) -> impl Future<Output = Result<usize, Self::Error>>;
}

#[derive(Debug)]
pub enum CreateDropError<ConsumerError, StoreErr> {
    EmptyDrop,
    StoreErr(StoreErr),
    ConsumerProblem(ConsumerError),
    Other,
}

#[derive(Debug)]
pub enum IngestDropError<ProducerError, StoreErr> {
    WrongNamespace,
    UnexpectedEndOfInput,
    ProducerProblem(ProducerError),
    UnauthorisedEntry(UnauthorisedWriteError),
    Other,
    EntryIngestion(EntryIngestionError<StoreErr>),
    PayloadIngestion(PayloadAppendError<ProducerError, StoreErr>),
}

impl<E, SE> From<UnauthorisedWriteError> for IngestDropError<E, SE> {
    fn from(value: UnauthorisedWriteError) -> Self {
        IngestDropError::UnauthorisedEntry(value)
    }
}

impl<F, E, O, SE> From<DecodeError<F, E, O>> for IngestDropError<E, SE> {
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
) -> Result<(), CreateDropError<EncryptedC::Error, StoreType::Error>>
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
        entries_count += store
            .count_area(&area)
            .await
            .map_err(CreateDropError::StoreErr)?;
        next_areas_vec.push(area);
    }

    if entries_count == 0 {
        return Err(CreateDropError::EmptyDrop);
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
            .map_err(CreateDropError::StoreErr)?;

        while let Ok(lengthy) = entry_producer.produce_item().await {
            let authed_entry = lengthy.entry();

            let has_full_payload = lengthy.available() == authed_entry.entry().payload_length();
            let has_different_subspace =
                authed_entry.entry().subspace_id() != prev_authed_entry.entry().subspace_id();
            let timestamp_larger_than_prev_timestamp =
                authed_entry.entry().timestamp() > prev_authed_entry.entry().timestamp();
            let timestamp_diff = authed_entry
                .entry()
                .timestamp()
                .abs_diff(prev_authed_entry.entry().timestamp());

            // And then a header.
            let mut header = 0;

            if has_full_payload {
                header |= 0b1000_0000;
            }

            if has_different_subspace {
                header |= 0b0100_0000;
            }

            if timestamp_larger_than_prev_timestamp {
                header |= 0b0010_0000;
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
                .relative_encode(&mut encrypted_consumer, prev_authed_entry.entry().path())
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

            // Encode auth token relative to previous authed entry and current entry.
            authed_entry
                .token()
                .relative_encode(
                    &mut encrypted_consumer,
                    &(prev_authed_entry, authed_entry.entry().clone()),
                )
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
                    .map_err(|err| match err {
                        willow_data_model::PayloadError::OperationError(err) => {
                            CreateDropError::StoreErr(err)
                        }
                        _ => {
                            panic!("Could not retrieve entry payload from store, even though we expected to be able to do so.")
                        }
                    })?;

                if let willow_data_model::Payload::Complete(mut payload) = payload {
                    bulk_pipe(&mut payload, &mut encrypted_consumer)
                        .await
                        .map_err(|err| match err {
                            ufotofu::PipeError::Producer(err) => CreateDropError::StoreErr(err),
                            ufotofu::PipeError::Consumer(err) => {
                                CreateDropError::ConsumerProblem(err)
                            }
                        })?;
                } else {
                    // We apparently had the full payload according to the lengthy.available, but the store says it is incomplete! Problems! no-one will be able to decode this!
                    panic!("The store said it had all the bytes for a given payload, but this turned out not to be true!")
                }
            }

            prev_authed_entry = authed_entry.clone();
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
>(
    producer: P,
    decrypt: DecryptFn,
    store: &StoreType,
) -> Result<(), IngestDropError<DecryptedP::Error, StoreType::Error>>
where
    P: Producer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    DecryptFn: Fn(P) -> DecryptedP,
    DecryptedP: BulkProducer<Item = u8>,
    Blame: From<N::ErrorReason> + From<S::ErrorReason> + From<PD::ErrorReason>,
    AT:,
{
    // do the inverse of https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol

    let mut decrypted_producer = decrypt(producer);

    let entries_count = CompactU64::decode(&mut decrypted_producer).await?;

    let namespace_id = N::decode(&mut decrypted_producer).await?;

    if &namespace_id != store.namespace_id() {
        return Err(IngestDropError::WrongNamespace);
    }

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

    for _ in 0..entries_count.0 {
        let header = decrypted_producer
            .produce_item()
            .await
            .map_err(|err| match err.reason {
                Either::Left(_) => IngestDropError::UnexpectedEndOfInput,
                Either::Right(err) => IngestDropError::ProducerProblem(err),
            })?;

        let payload_is_encoded = is_bitflagged(header, 0);
        let subspace_is_encoded = is_bitflagged(header, 1);
        let add_timestamp_diff = is_bitflagged(header, 2);
        let timestamp_diff_tag = Tag::from_raw(header, TagWidth::two(), 4);
        let payload_length_tag = Tag::from_raw(header, TagWidth::two(), 6);

        let subspace = if subspace_is_encoded {
            S::decode(&mut decrypted_producer).await?
        } else {
            prev_authed_entry.entry().subspace_id().clone()
        };

        let path = Path::relative_decode(&mut decrypted_producer, prev_authed_entry.entry().path())
            .await?;

        let timestamp_diff =
            CompactU64::relative_decode(&mut decrypted_producer, &timestamp_diff_tag).await?;

        let timestamp = if add_timestamp_diff {
            prev_authed_entry
                .entry()
                .timestamp()
                .checked_add(timestamp_diff.0)
                .ok_or(IngestDropError::Other)?
        } else {
            prev_authed_entry
                .entry()
                .timestamp()
                .checked_sub(timestamp_diff.0)
                .ok_or(IngestDropError::Other)?
        };

        let payload_length =
            CompactU64::relative_decode(&mut decrypted_producer, &payload_length_tag)
                .await?
                .0;

        let payload_digest = PD::decode(&mut decrypted_producer).await?;

        let entry = Entry::new(
            namespace_id.clone(),
            subspace,
            path,
            timestamp,
            payload_length,
            payload_digest,
        );

        let pair = (prev_authed_entry, entry);

        let token = AT::relative_decode(&mut decrypted_producer, &pair).await?;

        let authed_entry = AuthorisedEntry::new(pair.1, token)?;

        store
            .ingest_entry(
                authed_entry.clone(),
                false,
                willow_data_model::EntryOrigin::Local,
            )
            .await
            .map_err(IngestDropError::EntryIngestion)?;

        if payload_is_encoded {
            let mut payload_producer =
                ufotofu::producer::Limit::new(decrypted_producer, payload_length as usize);

            store
                .append_payload(
                    authed_entry.entry().subspace_id(),
                    authed_entry.entry().path(),
                    Some(authed_entry.entry().payload_digest().clone()),
                    &mut payload_producer,
                )
                .await
                .map_err(IngestDropError::PayloadIngestion)?;

            decrypted_producer = payload_producer.into_inner();
        }

        prev_authed_entry = authed_entry;
    }

    Ok(())
}
