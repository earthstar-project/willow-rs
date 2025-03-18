use either::Either;
use std::{convert::Infallible, marker::PhantomData};
use ufotofu_queues::Fixed;
use wb_async_utils::spsc::{new_spsc, Receiver, Sender, State};

use sled::{
    transaction::TransactionalTree, Db, Error as SledError, IVec, Result as SledResult,
    Transactional, Tree,
};
use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, Consumer, Producer,
};
use ufotofu_codec::{Decodable, Encodable, EncodableKnownSize, EncodableSync};
use ufotofu_codec_endian::U64BE;
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    AuthorisationToken, AuthorisedEntry, Component, Entry, EntryIngestionError,
    EntryIngestionSuccess, EventSenderError, ForgetPayloadError, LengthyAuthorisedEntry,
    NamespaceId, Path, PayloadAppendError, PayloadAppendSuccess, PayloadDigest, QueryIgnoreParams,
    QueryOrder, Store, StoreEvent, SubspaceId,
};

pub struct StoreSimpleSled<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    namespace_id: N,
    db: Db,
    event_packs: std::cell::RefCell<Vec<EventSubscriberPack<MCL, MCC, MPL, N, S, PD, AT>>>,
}

const ENTRY_TREE_KEY: [u8; 1] = [0b0000_0000];
const PAYLOAD_TREE_KEY: [u8; 1] = [0b0000_0001];
const MISC_TREE_KEY: [u8; 1] = [0b0000_0010];

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    pub fn new(namespace: &N, db: Db) -> Self
    where
        N: NamespaceId + EncodableKnownSize + EncodableSync,
    {
        // Write the namespace to the misc tree
        // Fail if there's already something there.
        todo!()
    }

    pub fn from_existing(db: Db) -> Self {
        // Check namespace from misc tree
        // Fail if there's nothing there
        todo!()
    }

    fn entry_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(ENTRY_TREE_KEY)
    }

    fn payload_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(PAYLOAD_TREE_KEY)
    }

    fn misc_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(MISC_TREE_KEY)
    }

    /// Return whether this store contains entries with paths that are prefixes of the given path and newer than the given timestamp
    async fn is_prefixed_by_newer(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        timestamp: u64,
    ) -> Result<Option<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, SimpleStoreSledError>
    where
        S: SubspaceId + EncodableSync + EncodableKnownSize + Decodable,
        PD: PayloadDigest + Decodable + EncodableSync + EncodableKnownSize,
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Decodable,
        S::ErrorReason: core::fmt::Debug,
        PD::ErrorReason: core::fmt::Debug,
        AT::ErrorReason: core::fmt::Debug,
    {
        // Iterate from subspace, just linearly
        // Create all prefixes of given path

        let tree = self.entry_tree()?;

        let prefix = subspace.sync_encode_into_vec();

        for (key, value) in tree.scan_prefix(&prefix).flatten() {
            let (other_subspace, other_path, other_timestamp) =
                decode_entry_key::<MCL, MCC, MPL, S>(&key).await;

            if timestamp < other_timestamp && path.is_prefixed_by(&other_path) {
                let (payload_length, payload_digest, authorisation_token) =
                    decode_entry_values(&value).await;

                let entry = Entry::new(
                    self.namespace_id.clone(),
                    other_subspace,
                    other_path,
                    timestamp,
                    payload_length,
                    payload_digest,
                );

                let authed = AuthorisedEntry::new_unchecked(entry, authorisation_token);

                return Ok(Some(authed));
            }
        }

        Ok(None)
    }

    fn flush(&self) -> Result<(), SimpleStoreSledError> {
        self.db.flush()?;

        Ok(())
    }

    /// Returns the next key and value from the given tree after the provided key AND which is prefixed by the given key.
    fn prefix_gt(
        &self,
        tree: &Tree,
        prefix: &[u8],
    ) -> Result<Option<(IVec, IVec)>, SimpleStoreSledError> {
        if let Some((key, value)) = tree.scan_prefix(prefix).flatten().next() {
            return Ok(Some((key, value)));
        }

        Ok(None)
    }
}

#[derive(Debug)]
pub struct SimpleStoreSledError {}

impl core::fmt::Display for SimpleStoreSledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Oh no!")
    }
}

impl core::error::Error for SimpleStoreSledError {}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    Store<MCL, MCC, MPL, N, S, PD, AT> for StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId + EncodableSync + EncodableKnownSize + Decodable,
    PD: PayloadDigest + Encodable + EncodableSync + EncodableKnownSize + Decodable,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Encodable + Decodable + Clone,
    S::ErrorReason: core::fmt::Debug,
    PD::ErrorReason: core::fmt::Debug,
    AT::ErrorReason: core::fmt::Debug,
{
    type FlushError = SimpleStoreSledError;

    type BulkIngestionError = SimpleStoreSledError;

    type OperationsError = SimpleStoreSledError;

    fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    async fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
    ) -> Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<Self::OperationsError>,
    > {
        let (entry, token) = authorised_entry.into_parts();

        if *entry.namespace_id() != self.namespace_id {
            panic!(
                "Store for {:?} tried to ingest entry of namespace {:?}",
                self.namespace_id,
                entry.namespace_id()
            )
        }

        if !token.is_authorised_write(&entry) {
            return Err(EntryIngestionError::NotAuthorised);
        }

        // Check if we have any newer entries with this prefix.
        match self
            .is_prefixed_by_newer(entry.subspace_id(), entry.path(), entry.timestamp())
            .await
        {
            Ok(Some(newer)) => {
                return Ok(EntryIngestionSuccess::Obsolete {
                    obsolete: AuthorisedEntry::new_unchecked(entry, token),
                    newer,
                })
            }
            Err(err) => return Err(EntryIngestionError::OperationsError(err)),
            Ok(None) => {
                // It's fine, continue.
            }
        }

        let entry_tree = self
            .entry_tree()
            .map_err(|_| EntryIngestionError::OperationsError(SimpleStoreSledError {}))?;

        let same_subspace_path_prefix_trailing_end =
            encode_subspace_path_key(entry.subspace_id(), entry.path(), false).await;

        let mut keys_to_prune: Vec<IVec> = Vec::new();

        for (key, value) in entry_tree
            .scan_prefix(&same_subspace_path_prefix_trailing_end)
            .flatten()
        {
            let (other_subspace, other_path, other_timestamp) =
                decode_entry_key::<MCL, MCC, MPL, S>(&key).await;

            let (other_payload_length, other_payload_digest, _other_authorisation_token) =
                decode_entry_values::<PD, AT>(&value).await;

            let other_entry = Entry::new(
                self.namespace_id.clone(),
                other_subspace,
                other_path,
                other_timestamp,
                other_payload_length,
                other_payload_digest,
            );

            if other_entry.is_newer_than(&entry) {
                continue;
            }

            // This should be pruned!

            if prevent_pruning {
                return Err(EntryIngestionError::PruningPrevented);
            }

            // Prune it!

            keys_to_prune.push(key);
        }

        let payload_tree = self
            .payload_tree()
            .map_err(|_| EntryIngestionError::OperationsError(SimpleStoreSledError {}))?;

        let key = encode_entry_key(entry.subspace_id(), entry.path(), entry.timestamp()).await;

        let value =
            encode_entry_values(entry.payload_length(), entry.payload_digest(), &token).await;

        let mut entry_batch = sled::Batch::default();
        let mut payload_batch = sled::Batch::default();

        for key in keys_to_prune {
            entry_batch.remove(&key);
            payload_batch.remove(&key);
        }
        entry_batch.insert(key.clone(), value);

        (&entry_tree, &payload_tree)
            .transaction(
                |(tx_entry, tx_payloads): &(TransactionalTree, TransactionalTree)| -> Result<
                    (),
                    sled::transaction::ConflictableTransactionError<SimpleStoreSledError>,
                > {
                    tx_entry.apply_batch(&entry_batch)?;
                    tx_payloads.apply_batch(&payload_batch)?;

                    Ok(())
                },
            )
            .map_err(|_| EntryIngestionError::OperationsError(SimpleStoreSledError {}))?;

        Ok(EntryIngestionSuccess::Success)
    }

    async fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        payload_source: &mut Producer,
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::OperationsError>>
    where
        Producer: BulkProducer<Item = u8, Error = PayloadSourceError>,
    {
        let entry_tree = self
            .entry_tree()
            .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;
        let payload_tree = self
            .payload_tree()
            .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;

        let exact_key = encode_subspace_path_key(subspace, path, true).await;

        let maybe_entry = self
            .prefix_gt(&entry_tree, &exact_key)
            .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;

        match maybe_entry {
            Some((key, value)) => {
                let (length, digest, _auth_token) = decode_entry_values::<PD, AT>(&value).await;

                if value.len() as u64 == length {
                    return Err(PayloadAppendError::TooManyBytes);
                }

                let existing_payload = payload_tree
                    .get(&key)
                    .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;

                let prefix = if let Some(payload) = existing_payload {
                    payload
                } else {
                    IVec::from(&[])
                };

                // Ingest

                let mut payload: Vec<u8> = Vec::from(prefix.as_ref());

                let mut received_payload_len = payload.len();

                let mut hasher = PD::hasher();

                loop {
                    // 3. Too many bytes ingested? Error.
                    if received_payload_len as u64 > length {
                        return Err(PayloadAppendError::TooManyBytes);
                    }

                    match payload_source.produce().await {
                        Ok(Either::Left(byte)) => {
                            payload.push(byte);
                            PD::write(&mut hasher, &[byte]);
                            received_payload_len += 1;
                        }
                        Ok(Either::Right(_)) => break,
                        Err(err) => {
                            return Err(PayloadAppendError::SourceError {
                                source_error: err,
                                total_length_now_available: todo!(),
                            })
                        }
                    }
                }

                if received_payload_len as u64 == length {
                    let computed_digest = PD::finish(&hasher);

                    if computed_digest != digest {
                        return Err(PayloadAppendError::DigestMismatch);
                    }

                    payload_tree
                        .insert(key, payload)
                        .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;

                    Ok(PayloadAppendSuccess::Completed)
                } else {
                    payload_tree
                        .insert(key, payload)
                        .map_err(|_| PayloadAppendError::OperationError(SimpleStoreSledError {}))?;
                    Ok(PayloadAppendSuccess::Appended)
                }
            }
            None => panic!("Tried to append a payload to an entry we do not have in the store."),
        }
    }

    async fn forget_entry(
        &self,
        subspace_id: &S,
        path: &willow_data_model::Path<MCL, MCC, MPL>,
    ) -> Result<(), Self::OperationsError> {
        let exact_key = encode_subspace_path_key(subspace_id, path, true).await;

        let entry_tree = self.entry_tree()?;
        let payload_tree = self.payload_tree()?;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;

        if let Some((key, value)) = maybe_entry {
            let (_length, _digest, _auth_token) = decode_entry_values::<PD, AT>(&value).await;

            (&entry_tree, &payload_tree)
                .transaction(
                    |(tx_entry, tx_payloads): &(TransactionalTree, TransactionalTree)| -> Result<
                        (),
                        sled::transaction::ConflictableTransactionError<SimpleStoreSledError>,
                    > {
                        tx_entry.remove(&key)?;
                        tx_payloads.remove(&key)?;

                        Ok(())
                    },
                )
                .map_err(|_| SimpleStoreSledError {})?;
        }

        Ok(())
    }

    async fn forget_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::OperationsError> {
        let entry_tree = self.entry_tree()?;
        let payload_tree = self.payload_tree()?;

        let mut entry_batch = sled::Batch::default();
        let mut payload_batch = sled::Batch::default();

        let mut forgotten_count = 0;

        let entry_iterator = match area.subspace() {
            AreaSubspace::Any => entry_tree.iter(),
            AreaSubspace::Id(subspace) => {
                let matching_subspace_path =
                    encode_subspace_path_key(subspace, area.path(), false).await;

                entry_tree.scan_prefix(&matching_subspace_path)
            }
        };

        for (key, value) in entry_iterator.flatten() {
            let (subspace, path, timestamp) = decode_entry_key(&key).await;
            let (_length, _digest, _token) = decode_entry_values::<PD, AT>(&value).await;

            let prefix_matches = if *area.subspace() == AreaSubspace::Any {
                path.is_prefixed_by(area.path())
            } else {
                // We know the path is a prefix because the iterator we used guarantees it.
                true
            };

            let timestamp_included = area.times().includes(&timestamp);

            let is_protected = match &protected {
                Some(protected_area) => {
                    protected_area.subspace().includes(&subspace)
                        && protected_area.path().is_prefix_of(&path)
                        && protected_area.times().includes(&timestamp)
                }
                None => false,
            };

            if !is_protected && prefix_matches && timestamp_included {
                // FORGET IT
                entry_batch.remove(&key);
                payload_batch.remove(&key);

                forgotten_count += 1;
            }
        }

        (&entry_tree, &payload_tree)
            .transaction(
                |(tx_entry, tx_payloads): &(TransactionalTree, TransactionalTree)| -> Result<
                    (),
                    sled::transaction::ConflictableTransactionError<SimpleStoreSledError>,
                > {
                    tx_entry.apply_batch(&entry_batch)?;
                    tx_payloads.apply_batch(&payload_batch)?;

                    Ok(())
                },
            )
            .map_err(|_| SimpleStoreSledError {})?;

        Ok(forgotten_count)
    }

    async fn forget_payload(
        &self,
        subspace_id: &S,
        path: &willow_data_model::Path<MCL, MCC, MPL>,
    ) -> Result<(), Self::OperationsError> {
        let payload_tree = self.payload_tree().map_err(|_| SimpleStoreSledError {})?;
        let exact_key = encode_subspace_path_key(subspace_id, path, true).await;

        let maybe_payload = self
            .prefix_gt(&payload_tree, &exact_key)
            .map_err(|_err| SimpleStoreSledError {})?;

        if let Some((key, _value)) = maybe_payload {
            payload_tree
                .remove(key)
                .map_err(|_err| SimpleStoreSledError {})?;
        }

        Ok(())
    }

    async fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::OperationsError> {
        let payload_tree = self.payload_tree()?;

        let mut payload_batch = sled::Batch::default();

        let mut forgotten_count = 0;

        let entry_iterator = match area.subspace() {
            AreaSubspace::Any => payload_tree.iter(),
            AreaSubspace::Id(subspace) => {
                let matching_subspace_path =
                    encode_subspace_path_key(subspace, area.path(), false).await;

                payload_tree.scan_prefix(&matching_subspace_path)
            }
        };

        for (key, value) in entry_iterator.flatten() {
            let (subspace, path, timestamp) = decode_entry_key(&key).await;
            let (_length, _digest, _token) = decode_entry_values::<PD, AT>(&value).await;

            let prefix_matches = if *area.subspace() == AreaSubspace::Any {
                path.is_prefixed_by(area.path())
            } else {
                // We know the path is a prefix because the iterator we used guarantees it.
                true
            };

            let timestamp_included = area.times().includes(&timestamp);

            let is_protected = match &protected {
                Some(protected_area) => {
                    protected_area.subspace().includes(&subspace)
                        && protected_area.path().is_prefix_of(&path)
                        && protected_area.times().includes(&timestamp)
                }
                None => false,
            };

            if !is_protected && prefix_matches && timestamp_included {
                // FORGET IT
                payload_batch.remove(&key);

                forgotten_count += 1;
            }
        }

        payload_tree
            .transaction(
                |tx_payloads| -> Result<
                    (),
                    sled::transaction::ConflictableTransactionError<SimpleStoreSledError>,
                > {
                    tx_payloads.apply_batch(&payload_batch)?;

                    Ok(())
                },
            )
            .map_err(|_| SimpleStoreSledError {})?;

        Ok(forgotten_count)
    }

    async fn flush(&self) -> Result<(), Self::FlushError> {
        self.flush()
    }

    async fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> Result<Option<impl Producer<Item = u8>>, Self::OperationsError> {
        let payload_tree = self.payload_tree()?;
        let exact_key = encode_subspace_path_key(subspace, path, true).await;

        let maybe_payload = self.prefix_gt(&payload_tree, &exact_key)?;

        if let Some((_key, value)) = maybe_payload {
            // Create a producer!
            Ok(Some(PayloadProducer::new(value)))
        } else {
            Ok(None)
        }
    }

    async fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: Option<QueryIgnoreParams>,
    ) -> Result<
        Option<willow_data_model::LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        Self::OperationsError,
    > {
        let exact_key = encode_subspace_path_key(subspace_id, path, true).await;

        let entry_tree = self.entry_tree()?;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;

        if let Some((key, value)) = maybe_entry {
            let (subspace, path, timestamp) = decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
            let (length, digest, token) = decode_entry_values::<PD, AT>(&value).await;

            let entry = Entry::new(
                self.namespace_id.clone(),
                subspace,
                path,
                timestamp,
                length,
                digest,
            );

            let authed_entry = AuthorisedEntry::new_unchecked(entry, token);

            let payload_tree = self.payload_tree()?;

            let maybe_payload = self.prefix_gt(&payload_tree, &exact_key)?;

            let payload_length = match maybe_payload {
                Some((_key, value)) => value.len(),
                None => 0,
            };

            match ignore {
                Some(params) => {
                    let is_empty = payload_length == 0;
                    let is_incomplete = is_empty || (payload_length as u64) < length;

                    if (params.ignore_incomplete_payloads && is_incomplete)
                        || (params.ignore_empty_payloads && is_empty)
                    {
                        return Ok(None);
                    }
                }
                None => {
                    return Ok(Some(LengthyAuthorisedEntry::new(
                        authed_entry,
                        payload_length as u64,
                    )));
                }
            };
        }

        Ok(None)
    }

    #[allow(refining_impl_trait)]
    async fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        order: &QueryOrder,
        reverse: bool,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Result<EntryProducer<MCL, MCC, MPL, N, S, PD, AT>, SimpleStoreSledError> {
        EntryProducer::new(self, area, order, reverse, ignore).await
    }

    fn subscribe_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Receiver<
        std::rc::Rc<
            State<
                Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
                (),
                EventSenderError<SimpleStoreSledError>,
            >,
        >,
        Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
        (),
        EventSenderError<SimpleStoreSledError>,
    > {
        let (pack, receiver) =
            EventSubscriberPack::<MCL, MCC, MPL, N, S, PD, AT>::new(area.clone(), ignore);

        self.event_packs.borrow_mut().push(pack);

        receiver
    }
}

/** Encode the key for a subspace and path **without** the timestamp. */
async fn encode_subspace_path_key<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S: SubspaceId + EncodableKnownSize + EncodableSync,
>(
    subspace: &S,
    path: &Path<MCL, MCC, MPL>,
    with_path_end: bool,
) -> Vec<u8> {
    let mut consumer: IntoVec<u8> = IntoVec::new();

    // Unwrap because IntoVec should not fail.
    subspace.encode(&mut consumer).await.unwrap();

    let component_count = path.component_count();

    for (i, component) in path.components().enumerate() {
        for byte in component.as_ref() {
            if *byte == 0 {
                // Unwrap because IntoVec should not fail.
                consumer.bulk_consume_full_slice(&[0, 2]).await.unwrap();
            } else {
                // Unwrap because IntoVec should not fail.
                consumer.consume(*byte).await.unwrap();
            }
        }

        if i < component_count - 1 || !with_path_end {
            // Unwrap because IntoVec should not fail.
            consumer.bulk_consume_full_slice(&[0, 1]).await.unwrap();
        } else {
            // Unwrap because IntoVec should not fail.
            consumer.bulk_consume_full_slice(&[0, 0]).await.unwrap();
        }
    }

    // No timestamp here!

    consumer.into_vec()
}

async fn encode_entry_key<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S: SubspaceId + EncodableKnownSize + EncodableSync,
>(
    subspace: &S,
    path: &Path<MCL, MCC, MPL>,
    timestamp: u64,
) -> Vec<u8> {
    let mut consumer: IntoVec<u8> = IntoVec::new();

    // Unwrap because IntoVec should not fail.
    subspace.encode(&mut consumer).await.unwrap();

    let component_count = path.component_count();

    for (i, component) in path.components().enumerate() {
        for byte in component.as_ref() {
            if *byte == 0 {
                // Unwrap because IntoVec should not fail.
                consumer.bulk_consume_full_slice(&[0, 2]).await.unwrap();
            } else {
                // Unwrap because IntoVec should not fail.
                consumer.consume(*byte).await.unwrap();
            }
        }

        if i < component_count - 1 {
            // Unwrap because IntoVec should not fail.
            consumer.bulk_consume_full_slice(&[0, 1]).await.unwrap();
        } else {
            // Unwrap because IntoVec should not fail.
            consumer.bulk_consume_full_slice(&[0, 0]).await.unwrap();
        }
    }

    // Unwrap because IntoVec should not fail.
    U64BE(timestamp).encode(&mut consumer).await.unwrap();

    consumer.into_vec()
}

async fn decode_entry_key<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S: SubspaceId + Decodable,
>(
    encoded: &IVec,
) -> (S, Path<MCL, MCC, MPL>, u64)
where
    S::ErrorReason: core::fmt::Debug,
{
    let mut producer = FromSlice::new(encoded);

    let subspace = S::decode(&mut producer).await.unwrap();

    let mut components_vecs: Vec<Vec<u8>> = Vec::new();

    loop {
        match component_bytes(&mut producer).await {
            Either::Left(bytes) => {
                components_vecs.push(bytes);
            }
            Either::Right(fin) => {
                // It's the last component.
                components_vecs.push(fin);
                break;
            }
        }
    }

    let mut components = components_vecs
        .iter()
        .map(|bytes| Component::new(bytes).expect("This should be fine! What could go wrong!"));

    let path: Path<MCL, MCC, MPL> =
        Path::new_from_iter(components_vecs.len(), &mut components).unwrap();

    let timestamp = U64BE::decode(&mut producer).await.unwrap().0;

    (subspace, path, timestamp)
}

async fn component_bytes<P: Producer<Item = u8>>(producer: &mut P) -> Either<Vec<u8>, Vec<u8>>
where
    P::Error: core::fmt::Debug,
    P::Final: core::fmt::Debug,
{
    let mut vec: Vec<u8> = Vec::new();
    let mut previous_was_zero = false;

    loop {
        let byte = producer.produce_item().await.unwrap();

        if byte == 0 {
            previous_was_zero = true
        } else if previous_was_zero && byte == 2 {
            // Append a zero.

            vec.push(0);
            previous_was_zero = false;
        } else if previous_was_zero && byte == 1 {
            // That's the end of the component. Yay.
            return Either::Left(vec);
        } else if previous_was_zero && byte == 0 {
            // That's the end of the component, and this is the last one! Stop here.
            return Either::Right(vec);
        } else {
            // Append to the component.
            vec.push(0);
            previous_was_zero = false;
        }
    }
}

async fn encode_entry_values<PD, AT>(
    payload_length: u64,
    payload_digest: &PD,
    auth_token: &AT,
) -> Vec<u8>
where
    PD: Encodable,
    AT: Encodable,
{
    let mut consumer: IntoVec<u8> = IntoVec::new();

    U64BE(payload_length).encode(&mut consumer).await.unwrap();
    payload_digest.encode(&mut consumer).await.unwrap();
    auth_token.encode(&mut consumer).await.unwrap();

    consumer.into_vec()
}

async fn decode_entry_values<PD, AT>(encoded: &IVec) -> (u64, PD, AT)
where
    PD: Decodable,
    AT: Decodable,
    PD::ErrorReason: core::fmt::Debug,
    AT::ErrorReason: core::fmt::Debug,
{
    let mut producer = FromSlice::new(encoded);

    let payload_length = U64BE::decode(&mut producer).await.unwrap().0;
    let payload_digest = PD::decode(&mut producer).await.unwrap();
    let auth_token = AT::decode(&mut producer).await.unwrap();

    (payload_length, payload_digest, auth_token)
}

impl From<SledError> for SimpleStoreSledError {
    fn from(_value: SledError) -> Self {
        SimpleStoreSledError {}
    }
}

pub struct PayloadProducer {
    produced: usize,
    ivec: IVec,
}

impl PayloadProducer {
    fn new(ivec: IVec) -> Self {
        Self { produced: 0, ivec }
    }
}

impl Producer for PayloadProducer {
    type Item = u8;

    type Final = ();

    type Error = Infallible;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if self.produced < self.ivec.len() {
            let byte = self.ivec[self.produced];

            Ok(Either::Left(byte))
        } else if self.produced == self.ivec.len() {
            return Ok(Either::Right(()));
        } else {
            panic!("You tried to produce more bytes than you could, but you claimed infallibity. You traitor. You fool.")
        }
    }
}

pub struct EntryProducer<'store, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    iter: sled::Iter,
    store: &'store StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>,
    ignore: Option<QueryIgnoreParams>,
    area: Area<MCL, MCC, MPL, S>,
    reverse: bool,
    _digest: PhantomData<PD>,
    _token: PhantomData<AT>,
}

impl<'store, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    EntryProducer<'store, MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId + EncodableKnownSize + EncodableSync,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    async fn new(
        store: &'store StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>,
        area: &Area<MCL, MCC, MPL, S>,
        order: &QueryOrder,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> Result<Self, SimpleStoreSledError> {
        let entry_tree = store.entry_tree()?;

        let entry_iterator = match area.subspace() {
            AreaSubspace::Any => entry_tree.iter(),
            AreaSubspace::Id(subspace) => {
                let matching_subspace_path =
                    encode_subspace_path_key(subspace, area.path(), false).await;

                entry_tree.scan_prefix(&matching_subspace_path)
            }
        };

        Ok(Self {
            iter: entry_iterator,
            area: area.clone(),
            ignore,
            store,
            reverse,
            _digest: PhantomData,
            _token: PhantomData,
        })
    }
}

impl<'store, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> Producer
    for EntryProducer<'store, MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable,
    S: SubspaceId + Decodable + EncodableKnownSize + EncodableSync,
    PD: PayloadDigest + Decodable,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Decodable,
    S::ErrorReason: std::fmt::Debug,
    PD::ErrorReason: std::fmt::Debug,
    AT::ErrorReason: std::fmt::Debug,
{
    type Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>;

    type Final = ();

    type Error = SimpleStoreSledError;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        loop {
            let result = if self.reverse {
                self.iter.next_back()
            } else {
                self.iter.next()
            };

            match result {
                Some(Ok((key, value))) => {
                    let (subspace, path, timestamp) =
                        decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
                    let (length, digest, token) = decode_entry_values::<PD, AT>(&value).await;

                    let exact_key = encode_subspace_path_key(&subspace, &path, true).await;

                    let entry = Entry::new(
                        self.store.namespace_id.clone(),
                        subspace,
                        path,
                        timestamp,
                        length,
                        digest,
                    );

                    if !self.area.includes_entry(&entry) {
                        continue;
                    }

                    let authed_entry = AuthorisedEntry::new_unchecked(entry, token);

                    let payload_tree = self.store.payload_tree()?;

                    let maybe_payload = self.store.prefix_gt(&payload_tree, &exact_key)?;

                    let payload_length = match maybe_payload {
                        Some((_key, value)) => value.len(),
                        None => 0,
                    };

                    match &self.ignore {
                        Some(params) => {
                            let is_empty = payload_length == 0;
                            let is_incomplete = is_empty || (payload_length as u64) < length;

                            if (params.ignore_incomplete_payloads && is_incomplete)
                                || (params.ignore_empty_payloads && is_empty)
                            {
                                continue;
                            }

                            return Ok(Either::Left(LengthyAuthorisedEntry::new(
                                authed_entry,
                                payload_length as u64,
                            )));
                        }
                        None => {
                            return Ok(Either::Left(LengthyAuthorisedEntry::new(
                                authed_entry,
                                payload_length as u64,
                            )));
                        }
                    };
                }
                Some(Err(_err)) => return Err(SimpleStoreSledError {}),
                None => return Ok(Either::Right(())),
            }
        }
    }
}

// Going to store a vec of these in the store
struct EventSubscriberPack<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    // Use these two to determine if we should send an event here
    area: Area<MCL, MCC, MPL, S>,
    ignore: Option<QueryIgnoreParams>,
    state: std::rc::Rc<
        State<
            Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
            (),
            EventSenderError<SimpleStoreSledError>,
        >,
    >,
    // We send events to this thing.
    sender: Sender<
        std::rc::Rc<
            State<
                Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
                (),
                EventSenderError<SimpleStoreSledError>,
            >,
        >,
        Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
        (),
        EventSenderError<SimpleStoreSledError>,
    >,
    // No sender here because we give ownership of that to the caller of the function?
}

impl<'state, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    EventSubscriberPack<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    fn new(
        area: Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> (
        Self,
        Receiver<
            std::rc::Rc<
                State<
                    Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
                    (),
                    EventSenderError<SimpleStoreSledError>,
                >,
            >,
            Fixed<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
            (),
            EventSenderError<SimpleStoreSledError>,
        >,
    ) {
        // TODO: Implement default on a wrapper type of StoreEvent
        let state = State::new(Fixed::<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>::new(256));
        let state_rc = std::rc::Rc::new(state);
        let (mut sender, mut receiver) = new_spsc(state_rc);

        let pack = Self {
            area,
            ignore,
            state: state_rc,
            sender,
        };

        (pack, receiver)
    }
}
