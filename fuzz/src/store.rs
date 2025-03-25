use std::{
    cell::{RefCell, RefMut},
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    ops::{Deref, DerefMut},
    rc::Rc,
    thread::current,
};

use either::Either::{Left, Right};
use meadowcap::SubspaceDelegation;
use ufotofu::{
    consumer::IntoVec,
    producer::{FromBoxedSlice, FromSlice},
    BulkConsumer, BulkProducer, Producer,
};
use ufotofu_queues::Fixed;
use wb_async_utils::spsc::{Sender, State};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    AuthorisationToken, AuthorisedEntry, BulkIngestionError, Component, Entry, EntryIngestionError,
    EntryIngestionSuccess, EntryOrigin, EventSenderError, LengthyAuthorisedEntry, NamespaceId,
    Path, PayloadAppendError, PayloadAppendSuccess, PayloadDigest, QueryIgnoreParams, QueryOrder,
    Store, StoreEvent, SubspaceId, Timestamp,
};

#[derive(Debug)]
pub struct ControlStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
    namespace: N,
    subspaces: RefCell<BTreeMap<S, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>>>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    ControlStore<MCL, MCC, MPL, N, S, PD, AT>
where
    S: Ord + Clone,
{
    fn get_or_create_subspace_store<'s>(
        &'s self,
        subspace_id: &S,
    ) -> RefMut<'s, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>> {
        let mut subspaces = self.subspaces.borrow_mut();

        if !subspaces.contains_key(subspace_id) {
            let _ = subspaces.insert(subspace_id.clone(), ControlSubspaceStore::new());
        }

        return RefMut::map(subspaces, |subspaces| {
            subspaces.get_mut(subspace_id).unwrap()
        });
    }
}

#[derive(Debug)]
pub struct ControlSubspaceStore<const MCL: usize, const MCC: usize, const MPL: usize, PD, AT> {
    entries: BTreeMap<Path<MCL, MCC, MPL>, ControlEntry<PD, AT>>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, PD, AT>
    ControlSubspaceStore<MCL, MCC, MPL, PD, AT>
{
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ControlEntry<PD, AT> {
    timestamp: Timestamp,
    payload_length: u64,
    payload_digest: PD,
    authorisation_token: AT,
    payload: Vec<u8>,
}

impl<PD: PayloadDigest, AT> ControlEntry<PD, AT> {
    /// https://willowprotocol.org/specs/data-model/index.html#entry_newer
    fn is_newer_than<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        S: SubspaceId,
    >(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool {
        if entry.timestamp() < self.timestamp {
            return true;
        } else if entry.timestamp() == self.timestamp
            && entry.payload_digest() < &self.payload_digest
        {
            return true;
        } else if entry.timestamp() == self.timestamp
            && entry.payload_digest() < &self.payload_digest
            && entry.payload_length() < self.payload_length
        {
            return true;
        } else {
            return false;
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    Store<MCL, MCC, MPL, N, S, PD, AT> for ControlStore<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Clone,
{
    type Error = Infallible;

    fn namespace_id(&self) -> &N {
        &self.namespace
    }

    async fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        origin: EntryOrigin,
    ) -> Result<EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>, EntryIngestionError<Self::Error>>
    {
        if self.namespace_id() != authorised_entry.entry().namespace_id() {
            panic!("Tried to ingest an entry into a store with a mismatching NamespaceId");
        }

        let mut subspace_store =
            self.get_or_create_subspace_store(&authorised_entry.entry().subspace_id());

        // Is the inserted entry redundant?
        for (path, entry) in subspace_store.deref().entries.iter() {
            if path.is_prefix_of(authorised_entry.entry().path())
                && entry.is_newer_than(authorised_entry.entry())
            {
                let subspace_id = authorised_entry.entry().subspace_id().clone();
                return Ok(EntryIngestionSuccess::Obsolete {
                    obsolete: authorised_entry,
                    newer: AuthorisedEntry::new(
                        Entry::new(
                            self.namespace.clone(),
                            subspace_id,
                            path.clone(),
                            entry.timestamp,
                            entry.payload_length,
                            entry.payload_digest.clone(),
                        ),
                        entry.authorisation_token.clone(),
                    )
                    .unwrap(),
                });
            }
        }

        // Does the inserted entry replace others?
        let prune_these: Vec<_> = subspace_store
            .entries
            .iter()
            .filter_map(|(path, entry)| {
                if authorised_entry.entry().path().is_prefix_of(path)
                    && !entry.is_newer_than(authorised_entry.entry())
                {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect();

        if prevent_pruning && !prune_these.is_empty() {
            return Err(EntryIngestionError::PruningPrevented);
        } else {
            for path_to_prune in prune_these {
                subspace_store.deref_mut().entries.remove(&path_to_prune);
            }

            return Ok(EntryIngestionSuccess::Success);
        }
    }

    async fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        payload_source: &mut Producer,
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::Error>>
    where
        Producer: ufotofu::BulkProducer<Item = u8, Error = PayloadSourceError>,
    {
        let mut subspace_store = self.get_or_create_subspace_store(subspace);

        match subspace_store.entries.get_mut(path) {
            None => panic!("Tried to append a payload for a non-existant entry."),
            Some(entry) => {
                let max_length = entry.payload_length;
                let mut current_length = entry.payload.len() as u64;

                while current_length < max_length {
                    match payload_source.expose_items().await {
                        Err(err) => {
                            return Err(PayloadAppendError::SourceError {
                                source_error: err,
                                total_length_now_available: current_length,
                            });
                        }
                        Ok(Left(bytes)) => {
                            current_length = current_length.saturating_add(bytes.len() as u64);
                            if current_length > max_length {
                                break;
                            }

                            entry.payload.extend_from_slice(bytes);
                        }
                        Ok(Right(_fin)) => break,
                    }
                }

                if current_length > max_length {
                    return Err(PayloadAppendError::TooManyBytes);
                } else if current_length == max_length {
                    let mut hasher = PD::hasher();
                    PD::write(&mut hasher, &entry.payload);

                    if PD::finish(&hasher) != entry.payload_digest {
                        return Err(PayloadAppendError::DigestMismatch);
                    } else {
                        return Ok(PayloadAppendSuccess::Completed);
                    }
                } else {
                    return Ok(PayloadAppendSuccess::Appended);
                }
            }
        }
    }

    async fn forget_entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> Result<(), Self::Error> {
        let mut subspace_store = self.get_or_create_subspace_store(subspace_id);
        subspace_store.entries.remove(path);
        Ok(())
    }

    async fn forget_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
        let mut candidates = vec![];

        let mut count = 0;

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if let Some(entry) = subspace_store.entries.get(path) {
                        candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                    }
                }
            }
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if let Some(entry) = subspace_store.entries.get(path) {
                            candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                        }
                    }
                }
            }
        }

        for candidate in candidates {
            if let Some(ref prot) = protected {
                if prot.includes_triplet(&candidate.0, &candidate.1, candidate.2.timestamp) {
                    continue;
                }
            }

            self.forget_entry(&candidate.0, &candidate.1).await?;
            count += 1;
        }

        Ok(count)
    }

    async fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> Result<(), Self::Error> {
        let mut subspace_store = self.get_or_create_subspace_store(subspace_id);
        match subspace_store.entries.get_mut(path) {
            None => panic!("Tried to forget the payload of an entry we do not have."),
            Some(entry) => {
                entry.payload.clear();
            }
        }
        Ok(())
    }

    async fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
        let mut candidates = vec![];

        let mut count = 0;

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if let Some(entry) = subspace_store.entries.get(path) {
                        candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                    }
                }
            }
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if let Some(entry) = subspace_store.entries.get(path) {
                            candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                        }
                    }
                }
            }
        }

        for candidate in candidates {
            if let Some(ref prot) = protected {
                if prot.includes_triplet(&candidate.0, &candidate.1, candidate.2.timestamp) {
                    continue;
                }
            }

            self.forget_payload(&candidate.0, &candidate.1).await?;
            count += 1;
        }

        Ok(count)
    }

    async fn flush(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> Result<Option<impl Producer<Item = u8>>, Self::Error> {
        let mut subspace_store = self.get_or_create_subspace_store(subspace);
        match subspace_store.entries.get_mut(path) {
            None => Ok(None),
            Some(entry) => Ok(Some(FromBoxedSlice::from_vec(entry.payload.clone()))),
        }
    }

    async fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: Option<QueryIgnoreParams>,
    ) -> Result<Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, Self::Error> {
        let subspace_store = self.get_or_create_subspace_store(subspace_id);
        match subspace_store.entries.get(path) {
            None => Ok(None),
            Some(entry) => {
                let available = entry.payload.len() as u64;

                if let Some(QueryIgnoreParams {
                    ignore_incomplete_payloads,
                    ignore_empty_payloads,
                }) = ignore
                {
                    if (ignore_empty_payloads && available == 0)
                        || (ignore_incomplete_payloads && available != entry.payload_length)
                    {
                        return Ok(None);
                    }
                }

                return Ok(Some(LengthyAuthorisedEntry::new(
                    AuthorisedEntry::new(
                        Entry::new(
                            self.namespace.clone(),
                            subspace_id.clone(),
                            path.clone(),
                            entry.timestamp,
                            entry.payload_length,
                            entry.payload_digest.clone(),
                        ),
                        entry.authorisation_token.clone(),
                    )
                    .unwrap(),
                    available,
                )));
            }
        }
    }

    async fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> Result<
        impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        Self::Error,
    > {
        let mut candidates = vec![];

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if let Some(entry) = subspace_store.entries.get(path) {
                        let available = entry.payload.len() as u64;

                        if let Some(QueryIgnoreParams {
                            ignore_incomplete_payloads,
                            ignore_empty_payloads,
                        }) = ignore
                        {
                            if (ignore_empty_payloads && available == 0)
                                || (ignore_incomplete_payloads && available != entry.payload_length)
                            {
                                continue;
                            } else {
                                candidates.push(LengthyAuthorisedEntry::new(
                                    AuthorisedEntry::new(
                                        Entry::new(
                                            self.namespace.clone(),
                                            subspace_id.clone(),
                                            path.clone(),
                                            entry.timestamp,
                                            entry.payload_length,
                                            entry.payload_digest.clone(),
                                        ),
                                        entry.authorisation_token.clone(),
                                    )
                                    .unwrap(),
                                    available,
                                ));
                            }
                        }
                    }
                }
            }
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if let Some(entry) = subspace_store.entries.get(path) {
                            let available = entry.payload.len() as u64;

                            if let Some(QueryIgnoreParams {
                                ignore_incomplete_payloads,
                                ignore_empty_payloads,
                            }) = ignore
                            {
                                if (ignore_empty_payloads && available == 0)
                                    || (ignore_incomplete_payloads
                                        && available != entry.payload_length)
                                {
                                    continue;
                                } else {
                                    candidates.push(LengthyAuthorisedEntry::new(
                                        AuthorisedEntry::new(
                                            Entry::new(
                                                self.namespace.clone(),
                                                subspace_id.clone(),
                                                path.clone(),
                                                entry.timestamp,
                                                entry.payload_length,
                                                entry.payload_digest.clone(),
                                            ),
                                            entry.authorisation_token.clone(),
                                        )
                                        .unwrap(),
                                        available,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        if reverse {
            candidates.reverse();
        }

        Ok(FromBoxedSlice::from_vec(candidates))
    }

    async fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<
        Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
        Error = EventSenderError<Self::Error>,
    > {
        panic!()
        // FromBoxedSlice::from_vec(vec![])
    }
}

pub enum StoreOp<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Clone,
{
    IngestEntry {
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        origin: EntryOrigin,
    },
    BulkIngestEntry {
        ingested: Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        prevent_pruning: bool,
        origin: EntryOrigin,
    },
    AppendPayload {
        subspace: S,
        path: Path<MCL, MCC, MPL>,
        data: Vec<u8>,
    },
    ForgetEntry {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
    },
    ForgetArea {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    ForgetPayload {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
    },
    ForgetAreaPayloads {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    GetPayload {
        subspace: S,
        path: Path<MCL, MCC, MPL>,
    },
    GetEntry {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
        ignore: Option<QueryIgnoreParams>,
    },
    QueryArea {
        area: Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    },
}

/// Panics if and only if the two stores do not exhibit equivalent behaviour upon executing the given `ops`.
pub async fn check_store_equality<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
    Store1,
    Store2,
>(
    store1: &mut Store1,
    store2: &mut Store2,
    ops: &[StoreOp<MCL, MCC, MPL, N, S, PD, AT>],
) where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Clone + std::fmt::Debug,
    Store1: Store<MCL, MCC, MPL, N, S, PD, AT>,
    Store2: Store<MCL, MCC, MPL, N, S, PD, AT>,
{
    let namespace_id = store1.namespace_id();
    assert_eq!(namespace_id, store2.namespace_id());

    for op in ops.iter() {
        match op {
            StoreOp::IngestEntry {
                authorised_entry,
                prevent_pruning,
                origin,
            } => {
                if authorised_entry.entry().namespace_id() != namespace_id {
                    continue;
                } else {
                    match (store1.ingest_entry(authorised_entry.clone(), *prevent_pruning, *origin).await, store1.ingest_entry(authorised_entry.clone(), *prevent_pruning, *origin).await) {
                        (Ok(yay1), Ok(yay2)) => assert_eq!(yay1, yay2),
                        (Err(EntryIngestionError::PruningPrevented), Err(EntryIngestionError::PruningPrevented)) | (Err(EntryIngestionError::NotAuthorised), Err(EntryIngestionError::NotAuthorised)) | (Err(EntryIngestionError::OperationsError(_)), Err(EntryIngestionError::OperationsError(_))) => continue,
                        (res1, res2) => panic!("IngestEntry: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}", res1, res2),
                        _ => todo!(),
                    }
                }
            }
            _ => todo!(),
        }
    }
}
