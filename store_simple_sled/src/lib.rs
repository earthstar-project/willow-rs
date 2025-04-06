use either::Either;
use std::{cell::RefCell, marker::PhantomData, rc::Rc};
use ufotofu::BufferedProducer;
use willow_data_model::{ForgetEntryError, ForgetPayloadError, PayloadError};

use sled::{
    transaction::{ConflictableTransactionError, TransactionError, TransactionalTree},
    Db, Error as SledError, IVec, Result as SledResult, Transactional, Tree,
};
use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, Consumer, Producer,
};
use ufotofu_codec::{Decodable, DecodableSync, Encodable, EncodableKnownSize, EncodableSync};
use ufotofu_codec_endian::U64BE;
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    AuthorisationToken, AuthorisedEntry, Component, Entry, EntryIngestionError,
    EntryIngestionSuccess, EntryOrigin, EventSystem, LengthyAuthorisedEntry, NamespaceId, Path,
    PayloadAppendError, PayloadAppendSuccess, PayloadDigest, QueryIgnoreParams, Store, StoreEvent,
    SubspaceId,
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
    _subspace: PhantomData<S>,
    _payload_digest: PhantomData<PD>,
    _auth_token: PhantomData<AT>,
    event_system: Rc<RefCell<EventSystem<MCL, MCC, MPL, N, S, PD, AT, SimpleStoreSledError>>>,
}

const ENTRY_TREE_KEY: [u8; 1] = [0b0000_0000];
const PAYLOAD_TREE_KEY: [u8; 1] = [0b0000_0001];
const MISC_TREE_KEY: [u8; 1] = [0b0000_0010];

const NAMESPACE_ID_KEY: [u8; 1] = [0b0000_0000];

#[derive(Debug)]
pub enum NewStoreSimpleSledError {
    // The DB has already been configured for another namespace.
    DbNotClean,
    StoreError(SimpleStoreSledError),
}

#[derive(Debug)]
pub enum ExistingStoreSimpleSledError {
    // The DB is not correctly configured for use.
    MalformedDb,
    StoreError(SimpleStoreSledError),
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + Decodable + DecodableSync,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    pub fn new(namespace: &N, db: Db) -> Result<Self, NewStoreSimpleSledError>
    where
        N: NamespaceId + EncodableKnownSize + EncodableSync,
    {
        Self::new_with_event_queue_capacity(namespace, db, 1024) // the 1024 is arbitrary, really
    }

    pub fn new_with_event_queue_capacity(
        namespace: &N,
        db: Db,
        capacity: usize,
    ) -> Result<Self, NewStoreSimpleSledError>
    where
        N: NamespaceId + EncodableKnownSize + EncodableSync,
    {
        let store = Self {
            db,
            namespace_id: namespace.clone(),
            _subspace: PhantomData,
            _payload_digest: PhantomData,
            _auth_token: PhantomData,
            event_system: Rc::new(RefCell::new(EventSystem::new(capacity))),
        };

        let misc_tree = store.misc_tree()?;

        if !misc_tree.is_empty() {
            return Err(NewStoreSimpleSledError::DbNotClean);
        }

        let namespace_encoded = namespace.sync_encode_into_vec();

        misc_tree.insert(NAMESPACE_ID_KEY, namespace_encoded)?;

        Ok(store)
    }

    pub fn from_existing(db: Db) -> Result<Self, ExistingStoreSimpleSledError> {
        Self::from_existing_with_event_queue_capacity(db, 1024) // the 1024 is arbitrary, really
    }

    pub fn from_existing_with_event_queue_capacity(
        db: Db,
        capacity: usize,
    ) -> Result<Self, ExistingStoreSimpleSledError> {
        let misc_tree = db.open_tree(MISC_TREE_KEY)?;

        let namespace_encoded = misc_tree
            .get(NAMESPACE_ID_KEY)?
            .ok_or(ExistingStoreSimpleSledError::MalformedDb)?;

        let namespace_id = N::sync_decode_from_slice(&namespace_encoded)
            .map_err(|_err| ExistingStoreSimpleSledError::MalformedDb)?;

        Ok(Self {
            namespace_id,
            db,
            _subspace: PhantomData,
            _payload_digest: PhantomData,
            _auth_token: PhantomData,
            event_system: Rc::new(RefCell::new(EventSystem::new(capacity))),
        })
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
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
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

        let prefix = entry.subspace_id().sync_encode_into_vec();

        for (key, value) in tree.scan_prefix(&prefix).flatten() {
            // println!("key: {:?}", key);

            let (other_subspace, other_path, other_timestamp) =
                decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
            let (payload_length, payload_digest, authorisation_token, _local_length) =
                decode_entry_values(&value).await;

            let other_entry = Entry::new(
                self.namespace_id.clone(),
                other_subspace,
                other_path,
                other_timestamp,
                payload_length,
                payload_digest,
            );

            if entry.path().is_prefixed_by(other_entry.path()) && other_entry.is_newer_than(entry) {
                let authed =
                    unsafe { AuthorisedEntry::new_unchecked(other_entry, authorisation_token) };

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

    /// Clear all data from the internal `sled::Db`
    pub fn clear(&self) -> Result<(), SimpleStoreSledError> {
        self.db.clear()?;
        self.flush()?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum SimpleStoreSledError {
    Sled(SledError),
    Transaction(TransactionError<()>),
    ConflictableTransaction(ConflictableTransactionError<()>),
}

impl core::fmt::Display for SimpleStoreSledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Oh no!")
    }
}

impl core::error::Error for SimpleStoreSledError {}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    Store<MCL, MCC, MPL, N, S, PD, AT> for StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + DecodableSync,
    S: SubspaceId + EncodableSync + EncodableKnownSize + Decodable,
    PD: PayloadDigest + Encodable + EncodableSync + EncodableKnownSize + Decodable,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Encodable + Decodable + Clone,
    S::ErrorReason: core::fmt::Debug,
    PD::ErrorReason: core::fmt::Debug,
    AT::ErrorReason: core::fmt::Debug,
{
    type Error = SimpleStoreSledError;

    fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    async fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        origin: EntryOrigin,
    ) -> Result<EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>, EntryIngestionError<Self::Error>>
    {
        let (entry, token) = authorised_entry.into_parts();

        if *entry.namespace_id() != self.namespace_id {
            panic!(
                "Store for {:?} tried to ingest entry of namespace {:?}",
                self.namespace_id,
                entry.namespace_id()
            )
        }

        // Check if we have any newer entries with this prefix.
        match self.is_prefixed_by_newer(&entry).await {
            Ok(Some(newer)) => {
                return Ok(EntryIngestionSuccess::Obsolete {
                    obsolete: unsafe { AuthorisedEntry::new_unchecked(entry, token) },
                    newer,
                })
            }
            Err(err) => return Err(EntryIngestionError::OperationsError(err)),
            Ok(None) => {
                // It's fine, continue.
            }
        }

        let entry_tree = self.entry_tree().map_err(SimpleStoreSledError::from)?;

        let same_subspace_path_prefix_trailing_end =
            encode_subspace_path_key(entry.subspace_id(), entry.path(), false).await;

        let mut keys_to_prune: Vec<IVec> = Vec::new();

        for (key, value) in entry_tree
            .scan_prefix(&same_subspace_path_prefix_trailing_end)
            .flatten()
        {
            let (other_subspace, other_path, other_timestamp) =
                decode_entry_key::<MCL, MCC, MPL, S>(&key).await;

            let (
                other_payload_length,
                other_payload_digest,
                _other_authorisation_token,
                _other_local_length,
            ) = decode_entry_values::<PD, AT>(&value).await;

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

        let payload_tree = self.payload_tree().map_err(SimpleStoreSledError::from)?;

        let key = encode_entry_key(entry.subspace_id(), entry.path(), entry.timestamp()).await;

        let value =
            encode_entry_values(entry.payload_length(), entry.payload_digest(), &token, 0).await;

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
                    ConflictableTransactionError<()>,
                > {
                    tx_entry.apply_batch(&entry_batch)?;
                    tx_payloads.apply_batch(&payload_batch)?;

                    Ok(())
                },
            )
            .map_err(SimpleStoreSledError::from)?;

        self.event_system
            .borrow_mut()
            .ingested_entry(AuthorisedEntry::new(entry, token).unwrap(), origin);

        Ok(EntryIngestionSuccess::Success)
    }

    async fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
        payload_source: &mut Producer,
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::Error>>
    where
        Producer: BulkProducer<Item = u8, Error = PayloadSourceError>,
    {
        let entry_tree = self.entry_tree().map_err(SimpleStoreSledError::from)?;
        let payload_tree = self.payload_tree().map_err(SimpleStoreSledError::from)?;

        let exact_key = encode_subspace_path_key(subspace, path, true).await;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;

        match maybe_entry {
            Some((entry_key, value)) => {
                let (subspace, path, timestamp) =
                    decode_entry_key::<MCL, MCC, MPL, S>(&entry_key).await;
                let (length, digest, auth_token, _local_length) =
                    decode_entry_values::<PD, AT>(&value).await;

                if let Some(expected) = expected_digest {
                    if expected != digest {
                        return Err(PayloadAppendError::WrongEntry);
                    }
                }

                let payload_key = encode_subspace_path_key(&subspace, &path, false).await;

                let existing_payload = payload_tree
                    .get(&payload_key)
                    .map_err(SimpleStoreSledError::from)?;

                let prefix = if let Some(payload) = existing_payload {
                    payload
                } else {
                    IVec::from(&[])
                };

                // Append the payload

                let mut payload: Vec<u8> = Vec::from(prefix.as_ref());
                let mut received_payload_len = payload.len();
                let mut hasher = PD::hasher();

                // Make sure the prefix is hashed too.
                PD::write(&mut hasher, &prefix);

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
                            // Update with what we were able to produce.
                            let new_value = encode_entry_values(
                                length,
                                &digest,
                                &auth_token,
                                received_payload_len as u64,
                            )
                            .await;

                            let mut entry_batch = sled::Batch::default();
                            let mut payload_batch = sled::Batch::default();

                            entry_batch.insert(entry_key, new_value);
                            payload_batch.insert(payload_key, payload);

                            (&entry_tree, &payload_tree)
                                .transaction(
                                    |(tx_entry, tx_payloads): &(
                                        TransactionalTree,
                                        TransactionalTree,
                                    )|
                                     -> Result<
                                        (),
                                        sled::transaction::ConflictableTransactionError<()>,
                                    > {
                                        tx_entry.apply_batch(&entry_batch)?;
                                        tx_payloads.apply_batch(&payload_batch)?;

                                        Ok(())
                                    },
                                )
                                .map_err(SimpleStoreSledError::from)?;

                            let entry = Entry::new(
                                self.namespace_id.clone(),
                                subspace,
                                path,
                                timestamp,
                                length,
                                digest,
                            );

                            let authy_entry =
                                unsafe { AuthorisedEntry::new_unchecked(entry, auth_token) };

                            self.event_system.borrow_mut().appended_payload(
                                LengthyAuthorisedEntry::new(
                                    authy_entry,
                                    received_payload_len as u64,
                                ),
                            );

                            return Err(PayloadAppendError::SourceError {
                                source_error: err,
                                total_length_now_available: received_payload_len as u64,
                            });
                        }
                    }
                }

                let authed_entry = unsafe {
                    AuthorisedEntry::new_unchecked(
                        Entry::new(
                            self.namespace_id.clone(),
                            subspace,
                            path,
                            timestamp,
                            length,
                            digest,
                        ),
                        auth_token,
                    )
                };

                let lengthy_entry =
                    LengthyAuthorisedEntry::new(authed_entry, received_payload_len as u64);

                let new_value = encode_entry_values(
                    length,
                    lengthy_entry.entry().entry().payload_digest(),
                    lengthy_entry.entry().token(),
                    received_payload_len as u64,
                )
                .await;

                let mut entry_batch = sled::Batch::default();
                let mut payload_batch = sled::Batch::default();

                entry_batch.insert(entry_key, new_value);
                payload_batch.insert(payload_key, payload);

                if received_payload_len as u64 == length {
                    let computed_digest = PD::finish(&hasher);

                    if computed_digest != *lengthy_entry.entry().entry().payload_digest() {
                        return Err(PayloadAppendError::DigestMismatch);
                    }

                    (&entry_tree, &payload_tree)
                    .transaction(
                        |(tx_entry, tx_payloads): &(
                            TransactionalTree,
                            TransactionalTree,
                        )|
                         -> Result<
                            (),
                            sled::transaction::ConflictableTransactionError<
                                (),
                            >,
                        > {
                            tx_entry.apply_batch(&entry_batch)?;
                            tx_payloads.apply_batch(&payload_batch)?;
                            Ok(())
                        },
                    )
                    .map_err(|err| {
                        SimpleStoreSledError::from(err)
                    })?;

                    self.event_system
                        .borrow_mut()
                        .appended_payload(lengthy_entry);

                    Ok(PayloadAppendSuccess::Completed)
                } else {
                    (&entry_tree, &payload_tree)
                    .transaction(
                        |(tx_entry, tx_payloads): &(
                            TransactionalTree,
                            TransactionalTree,
                        )|
                         -> Result<
                            (),
                            sled::transaction::ConflictableTransactionError<()>,
                        > {
                            tx_entry.apply_batch(&entry_batch)?;
                            tx_payloads.apply_batch(&payload_batch)?;
                            Ok(())
                        },
                    )
                    .map_err(|err| {
                        SimpleStoreSledError::from(err)
                    })?;

                    self.event_system
                        .borrow_mut()
                        .appended_payload(lengthy_entry);

                    Ok(PayloadAppendSuccess::Appended)
                }
            }
            None => Err(PayloadAppendError::NoSuchEntry),
        }
    }

    async fn forget_entry(
        &self,
        subspace_id: &S,
        path: &willow_data_model::Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> Result<(), ForgetEntryError<Self::Error>> {
        let exact_key = encode_subspace_path_key(subspace_id, path, true).await;

        let entry_tree = self.entry_tree().map_err(SimpleStoreSledError::from)?;
        let payload_tree = self.payload_tree().map_err(SimpleStoreSledError::from)?;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;

        if let Some((key, value)) = maybe_entry {
            let (subspace_id, path, timestamp) = decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
            let (length, digest, auth_token, local_length) =
                decode_entry_values::<PD, AT>(&value).await;

            if let Some(expected) = expected_digest {
                if expected != digest {
                    return Err(ForgetEntryError::WrongEntry);
                }
            }

            (&entry_tree, &payload_tree)
                .transaction(
                    |(tx_entry, tx_payloads): &(TransactionalTree, TransactionalTree)| -> Result<
                        (),
                        sled::transaction::ConflictableTransactionError<()>,
                    > {
                        tx_entry.remove(&key)?;
                        tx_payloads.remove(&key)?;

                        Ok(())
                    },
                ) .map_err(SimpleStoreSledError::from)?;

            let entry = Entry::new(
                self.namespace_id.clone(),
                subspace_id,
                path,
                timestamp,
                length,
                digest,
            );

            // We can do this because the token comes from within our store (where it was vetted prior to ingestion)
            let authy_entry = unsafe { AuthorisedEntry::new_unchecked(entry, auth_token) };

            self.event_system
                .borrow_mut()
                .forgot_entry(LengthyAuthorisedEntry::new(authy_entry, local_length));
        }

        Ok(())
    }

    async fn forget_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
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
            let (_length, _digest, _token, _local_length) =
                decode_entry_values::<PD, AT>(&value).await;

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
                    sled::transaction::ConflictableTransactionError<()>,
                > {
                    tx_entry.apply_batch(&entry_batch)?;
                    tx_payloads.apply_batch(&payload_batch)?;

                    Ok(())
                },
            )?;

        self.event_system
            .borrow_mut()
            .forgot_area(area.clone(), protected.cloned());

        Ok(forgotten_count)
    }

    async fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> Result<(), ForgetPayloadError<Self::Error>> {
        let payload_tree = self.payload_tree().map_err(SimpleStoreSledError::from)?;

        let payload_key = encode_subspace_path_key(subspace_id, path, false).await;

        let maybe_payload = self.prefix_gt(&payload_tree, &payload_key)?;

        let entry_tree = self.entry_tree().map_err(SimpleStoreSledError::from)?;

        let entry_key_partial = encode_subspace_path_key(subspace_id, path, true).await;
        let maybe_entry = self.prefix_gt(&entry_tree, &entry_key_partial)?;

        match (maybe_entry, maybe_payload) {
            (Some((entry_key, entry_value)), Some((payload_key, _payload_value))) => {
                let (subspace, path, timestamp) =
                    decode_entry_key::<MCL, MCC, MPL, S>(&entry_key).await;
                let (length, digest, auth_token, local_length) =
                    decode_entry_values::<PD, AT>(&entry_value).await;

                if let Some(expected) = expected_digest {
                    if expected != digest {
                        return Err(ForgetPayloadError::WrongEntry);
                    }
                }

                let new_key_value = encode_entry_values(length, &digest, &auth_token, 0).await;

                (&entry_tree, &payload_tree).transaction(
                    |(entry_tx, payload_tx): &(TransactionalTree, TransactionalTree)| -> Result<
                        (),
                        sled::transaction::ConflictableTransactionError<()>,
                    > {
                        payload_tx.remove(&payload_key)?;
                        entry_tx.insert(&entry_key, new_key_value.clone())?;

                        Ok(())
                    },
                ).map_err(SimpleStoreSledError::from)?;

                let entry = Entry::new(
                    self.namespace_id.clone(),
                    subspace,
                    path,
                    timestamp,
                    length,
                    digest,
                );

                let authy_entry = unsafe { AuthorisedEntry::new_unchecked(entry, auth_token) };

                self.event_system
                    .borrow_mut()
                    .forgot_payload(LengthyAuthorisedEntry::new(authy_entry, local_length));

                Ok(())
            }
            (Some((_entry_key, entry_value)), None) => {
                if let Some(expected) = expected_digest {
                    let (_length, digest, _auth_token, _local_length) =
                    decode_entry_values::<PD, AT>(&entry_value).await;
                   
                    if expected != digest {
                        return Err(ForgetPayloadError::WrongEntry);
                    }
                }
                
                Ok(())
            },
            (None, None) => Err(ForgetPayloadError::NoSuchEntry),
            (None, Some(_)) => panic!("StoreSimpleSled is storing a payload with no corresponding entry, which indicates an implementation error!"),
        }
    }

    async fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
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
            let (length, digest, token, _local_length) =
                decode_entry_values::<PD, AT>(&value).await;

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
                let entry_values = encode_entry_values(length, &digest, &token, 0).await;

                entry_batch.insert(&key, entry_values);
                payload_batch.remove(&key);

                forgotten_count += 1;
            }
        }

        (&entry_tree, &payload_tree).transaction(
            |(tx_entry, tx_payloads): &(TransactionalTree, TransactionalTree)| -> Result<
                (),
                sled::transaction::ConflictableTransactionError<()>,
            > {
                tx_entry.apply_batch(&entry_batch)?;
                tx_payloads.apply_batch(&payload_batch)?;

                Ok(())
            },
        )?;

        self.event_system
            .borrow_mut()
            .forgot_area(area.clone(), protected.cloned());

        Ok(forgotten_count)
    }

    async fn flush(&self) -> Result<(), Self::Error> {
        self.flush()
    }

    async fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> Result<
        Option<impl BulkProducer<Item = u8, Final = (), Error = Self::Error>>,
        PayloadError<Self::Error>,
    > {
        let entry_tree = self.entry_tree().map_err(SimpleStoreSledError::from)?;
        let payload_tree = self.payload_tree().map_err(SimpleStoreSledError::from)?;
        let exact_key = encode_subspace_path_key(subspace, path, true).await;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;
        let maybe_payload = self.prefix_gt(&payload_tree, &exact_key)?;

        match (maybe_entry, maybe_payload) {
            (Some((_entry_key, entry_value)), Some((_payload_key, payload_value))) => {
                let (_length, digest, _token, _local_length) =
                decode_entry_values::<PD, AT>(&entry_value).await;
                
                if let Some(expected) = expected_digest {
                    if expected != digest {
                        return Err(PayloadError::WrongEntry);
                    }
                }
                
                Ok(Some(PayloadProducer::new(payload_value)))
            },
            (Some((_entry_key, entry_value)), None) => {
                // check expected digest.
                let (_length, digest, _token, _local_length) =
                decode_entry_values::<PD, AT>(&entry_value).await;
                
                if let Some(expected) = expected_digest {
                    if expected != digest {
                        return Err(PayloadError::WrongEntry);
                    }
                }
                
                Ok(None)
                
            },
            (None, None) => Ok(None),
            (None, Some(_)) => panic!("Holding a payload for which there is no corresponding entry, this is bad!"),
        }
    }

    async fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: QueryIgnoreParams,
    ) -> Result<
        Option<willow_data_model::LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        Self::Error,
    > {
        let exact_key = encode_subspace_path_key(subspace_id, path, true).await;

        let entry_tree = self.entry_tree()?;

        let maybe_entry = self.prefix_gt(&entry_tree, &exact_key)?;

        if let Some((key, value)) = maybe_entry {
            let (subspace, path, timestamp) = decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
            let (length, digest, token, local_length) = decode_entry_values::<PD, AT>(&value).await;

            let entry = Entry::new(
                self.namespace_id.clone(),
                subspace,
                path,
                timestamp,
                length,
                digest,
            );

            let authed_entry = unsafe { AuthorisedEntry::new_unchecked(entry, token) };

            let payload_tree = self.payload_tree()?;

            let maybe_payload = self.prefix_gt(&payload_tree, &exact_key)?;

            let payload_length = match maybe_payload {
                Some((_key, value)) => value.len(),
                None => 0,
            };

            let payload_is_empty_string = length == 0;
            let is_incomplete = local_length < length;

            if (ignore.ignore_incomplete_payloads && is_incomplete)
                || (ignore.ignore_empty_payloads && payload_is_empty_string)
            {   
                return Ok(None);
            } else {
                return Ok(Some(LengthyAuthorisedEntry::new(
                    authed_entry,
                    payload_length as u64,
                )));
            }
        }

        Ok(None)
    }

    async fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> Result<
        impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, Final = ()>,
        Self::Error,
    > {
        EntryProducer::new(self, area, ignore).await
    }

    async fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>, Final = (), Error = Self::Error>
    {
        EventSystem::add_subscription(self.event_system.clone(), area.clone(), ignore)
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

    for component in path.components() {
        for byte in component.as_ref() {
            if *byte == 0 {
                // Unwrap because IntoVec should not fail.
                consumer.bulk_consume_full_slice(&[0, 2]).await.unwrap();
            } else {
                // Unwrap because IntoVec should not fail.
                consumer.consume(*byte).await.unwrap();
            }
        }

        // Unwrap because IntoVec should not fail.
        consumer.bulk_consume_full_slice(&[0, 1]).await.unwrap();
    }

    // Unwrap because IntoVec should not fail.
    if with_path_end {
        consumer.bulk_consume_full_slice(&[0, 0]).await.unwrap();
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

    for component in path.components() {
        for byte in component.as_ref() {
            if *byte == 0 {
                // Unwrap because IntoVec should not fail.
                consumer.bulk_consume_full_slice(&[0, 2]).await.unwrap();
            } else {
                // Unwrap because IntoVec should not fail.
                consumer.consume(*byte).await.unwrap();
            }
        }

        // Unwrap because IntoVec should not fail.
        consumer.bulk_consume_full_slice(&[0, 1]).await.unwrap();
    }

    // Unwrap because IntoVec should not fail.
    consumer.bulk_consume_full_slice(&[0, 0]).await.unwrap();

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

    while let Some(bytes) = component_bytes(&mut producer).await {
        components_vecs.push(bytes);
    }

    let mut components = components_vecs
        .iter()
        .map(|bytes| Component::new(bytes).expect("Component was unexpectedly longer than MCL."));

    let total_len = components.clone().fold(0, |acc, comp| acc + comp.len());

    let path: Path<MCL, MCC, MPL> = Path::new_from_iter(total_len, &mut components).unwrap();

    let timestamp = U64BE::decode(&mut producer).await.unwrap().0;

    (subspace, path, timestamp)
}

async fn component_bytes<P: Producer<Item = u8>>(producer: &mut P) -> Option<Vec<u8>>
where
    P::Error: core::fmt::Debug,
    P::Final: core::fmt::Debug,
{
    let mut vec: Vec<u8> = Vec::new();
    let mut previous_was_zero = false;

    loop {
        match producer.produce().await {
            Ok(Either::Left(byte)) => {
                if !previous_was_zero && byte == 0 {
                    previous_was_zero = true
                } else if previous_was_zero && byte == 2 {
                    // Append a zero.

                    vec.push(0);
                    previous_was_zero = false;
                } else if previous_was_zero && byte == 1 {
                    // That's the end of this component..
                    return Some(vec);
                } else if previous_was_zero && byte == 0 {
                    // That's the end of the path.
                    return None;
                } else {
                    // Append to the component.
                    vec.push(byte);
                    previous_was_zero = false;
                }
            }
            Ok(Either::Right(_)) => {
                if previous_was_zero {
                    panic!("Unterminated escaped key!")
                }

                return None;
            }
            Err(err) => panic!("Unexpected error: {:?}", err),
        }
    }
}

async fn encode_entry_values<PD, AT>(
    payload_length: u64,
    payload_digest: &PD,
    auth_token: &AT,
    local_length: u64,
) -> Vec<u8>
where
    PD: Encodable,
    AT: Encodable,
{
    let mut consumer: IntoVec<u8> = IntoVec::new();

    U64BE(payload_length).encode(&mut consumer).await.unwrap();
    payload_digest.encode(&mut consumer).await.unwrap();
    auth_token.encode(&mut consumer).await.unwrap();
    U64BE(local_length).encode(&mut consumer).await.unwrap();

    consumer.into_vec()
}

async fn decode_entry_values<PD, AT>(encoded: &IVec) -> (u64, PD, AT, u64)
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
    let local_length = U64BE::decode(&mut producer).await.unwrap().0;

    (payload_length, payload_digest, auth_token, local_length)
}

impl From<SledError> for SimpleStoreSledError {
    fn from(value: SledError) -> Self {
        SimpleStoreSledError::Sled(value)
    }
}

impl From<ConflictableTransactionError<()>> for SimpleStoreSledError {
    fn from(value: ConflictableTransactionError<()>) -> Self {
        SimpleStoreSledError::ConflictableTransaction(value)
    }
}

impl From<TransactionError<()>> for SimpleStoreSledError {
    fn from(value: TransactionError<()>) -> Self {
        SimpleStoreSledError::Transaction(value)
    }
}

impl From<SledError> for NewStoreSimpleSledError {
    fn from(value: SledError) -> Self {
        Self::StoreError(SimpleStoreSledError::from(value))
    }
}

impl From<SledError> for ExistingStoreSimpleSledError {
    fn from(value: SledError) -> Self {
        Self::StoreError(SimpleStoreSledError::from(value))
    }
}

impl From<SimpleStoreSledError> for EntryIngestionError<SimpleStoreSledError> {
    fn from(val: SimpleStoreSledError) -> Self {
        EntryIngestionError::OperationsError(val)
    }
}

impl<PSE> From<SimpleStoreSledError> for PayloadAppendError<PSE, SimpleStoreSledError> {
    fn from(val: SimpleStoreSledError) -> Self {
        PayloadAppendError::OperationError(val)
    }
}

impl From<SimpleStoreSledError> for ForgetEntryError<SimpleStoreSledError> {
    fn from(value: SimpleStoreSledError) -> Self {
        Self::OperationError(value)
    }
}

impl From<SimpleStoreSledError> for ForgetPayloadError<SimpleStoreSledError> {
    fn from(value: SimpleStoreSledError) -> Self {
        Self::OperationError(value)
    }
}

impl From<SimpleStoreSledError> for PayloadError<SimpleStoreSledError> {
    fn from(value: SimpleStoreSledError) -> Self {
        Self::OperationError(value)
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

    type Error = SimpleStoreSledError;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        match self.produced.cmp(&self.ivec.len()) {
            std::cmp::Ordering::Less => {
                let byte = self.ivec[self.produced];
                Ok(Either::Left(byte))
            },
            std::cmp::Ordering::Equal =>  Ok(Either::Right(())),
            std::cmp::Ordering::Greater => unreachable!("You tried to produce more bytes than you could, but you claimed infallibity. You traitor. You fool."),
        }
    }
}

impl BufferedProducer for PayloadProducer {
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl BulkProducer for PayloadProducer {
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        let slice = &self.ivec[self.produced..];
        if slice.is_empty() {
            Ok(Either::Right(()))
        } else {
            Ok(Either::Left(slice))
        }
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.produced += amount;

        Ok(())
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
    ignore: QueryIgnoreParams,
    area: Area<MCL, MCC, MPL, S>,
}

impl<'store, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    EntryProducer<'store, MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + EncodableKnownSize + DecodableSync,
    S: SubspaceId + EncodableKnownSize + EncodableSync,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    async fn new(
        store: &'store StoreSimpleSled<MCL, MCC, MPL, N, S, PD, AT>,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
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
            let result = self.iter.next();

            match result {
                Some(Ok((key, value))) => {
                    let (subspace, path, timestamp) =
                        decode_entry_key::<MCL, MCC, MPL, S>(&key).await;
                    let (length, digest, token, local_length) =
                        decode_entry_values::<PD, AT>(&value).await;

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

                    let authed_entry = unsafe { AuthorisedEntry::new_unchecked(entry, token) };

                    let is_empty_string = length == 0;
                    let is_incomplete = local_length < length;

                    if (self.ignore.ignore_incomplete_payloads && is_incomplete)
                        || (self.ignore.ignore_empty_payloads && is_empty_string)
                    {
                        continue;
                    }

                    return Ok(Either::Left(LengthyAuthorisedEntry::new(
                        authed_entry,
                        local_length,
                    )));
                }
                Some(Err(err)) => return Err(SimpleStoreSledError::from(err)),
                None => return Ok(Either::Right(())),
            }
        }
    }
}
