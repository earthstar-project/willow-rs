use std::{
    cell::{RefCell, RefMut},
    collections::BTreeMap,
    convert::Infallible,
    ops::{Deref, DerefMut},
    thread::current,
};

use either::Either::{Left, Right};
use meadowcap::SubspaceDelegation;
use ufotofu::{consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer, Producer};
use willow_data_model::{
    grouping::Area, AuthorisationToken, AuthorisedEntry, BulkIngestionError, Component, Entry,
    EntryIngestionError, EntryIngestionSuccess, ForgetPayloadError, LengthyAuthorisedEntry,
    NamespaceId, Path, PayloadAppendError, PayloadAppendSuccess, PayloadDigest, Store, StoreEvent,
    SubspaceId, Timestamp,
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

#[derive(Debug)]
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
    type FlushError = Infallible;

    type BulkIngestionError = Infallible;

    type OperationsError = Infallible;

    fn namespace_id(&self) -> &N {
        &self.namespace
    }

    async fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
    ) -> Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<Self::OperationsError>,
    > {
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
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::OperationsError>>
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
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
    ) -> Result<(), Self::OperationsError> {
        todo!()
    }

    async fn forget_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
    ) -> Result<u64, Self::OperationsError> {
        todo!()
    }

    async fn forget_payload(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
    ) -> Result<(), ForgetPayloadError<Self::OperationsError>> {
        todo!()
    }

    async fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::OperationsError> {
        todo!()
    }

    async fn flush(&self) -> Result<(), Self::FlushError> {
        todo!()
    }

    async fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> Result<Option<impl Producer<Item = u8>>, Self::OperationsError> {
        todo!()
    }

    async fn entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Result<Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, Self::OperationsError>
    {
        todo!()
    }

    fn query_area(
        &self,
        area: &willow_data_model::grouping::AreaOfInterest<MCL, MCC, MPL, S>,
        order: &willow_data_model::QueryOrder,
        reverse: bool,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> FromSlice<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }

    fn subscribe_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> FromSlice<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }
}
