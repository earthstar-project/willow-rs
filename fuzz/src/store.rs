use std::{
    cell::{RefCell, RefMut},
    collections::BTreeMap,
    convert::Infallible,
    ops::{Deref, DerefMut},
};

use meadowcap::SubspaceDelegation;
use ufotofu::{consumer::IntoVec, producer::FromSlice, BulkConsumer, BulkProducer};
use willow_data_model::{
    AuthorisationToken, AuthorisedEntry, BulkIngestionError, Component, Entry, EntryIngestionError,
    EntryIngestionSuccess, LengthyAuthorisedEntry, NamespaceId, Path, PayloadAppendError,
    PayloadAppendSuccess, PayloadDigest, Store, StoreEvent, SubspaceId, Timestamp,
};

#[derive(Debug)]
pub struct ControlStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
    namespace: N,
    subspaces: RefCell<BTreeMap<S, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>>>,
    payloads: RefCell<BTreeMap<PD, Vec<u8>>>,
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
        EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, Self::OperationsError>,
    > {
        if self.namespace_id() != authorised_entry.entry().namespace_id() {
            return Err(EntryIngestionError::WrongNamespace(authorised_entry));
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

    async fn append_payload<Producer>(
        &self,
        expected_digest: &PD,
        expected_size: u64,
        payload_source: &mut Producer,
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<Self::OperationsError>>
    where
        Producer: ufotofu::BulkProducer<Item = u8>,
    {
        match self.payloads.borrow_mut().get_mut(expected_digest) {
            None => todo!(),
            Some(prior_bytes) => {
                // Good thing this implementation is for testing purposes only...
                let mut owned_payload = prior_bytes.clone();
                let mut owned_payload_consumer = IntoVec::from_vec(owned_payload);
                todo!()
            }
        }
        // self.get_or_create_subspace_store();
        todo!()
    }

    async fn forget_entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        traceless: bool,
    ) -> Result<(), Self::OperationsError> {
        todo!()
    }

    async fn forget_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> Result<u64, Self::OperationsError> {
        todo!()
    }

    async fn forget_payload(
        path: &Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> Result<(), willow_data_model::ForgetPayloadError> {
        todo!()
    }

    async fn force_forget_payload(
        path: &Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> Result<(), willow_data_model::NoSuchEntryError> {
        todo!()
    }

    async fn forget_area_payloads(
        &self,
        area: &willow_data_model::grouping::AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> Vec<PD> {
        todo!()
    }

    async fn force_forget_area_payloads(
        &self,
        area: &willow_data_model::grouping::AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> Vec<PD> {
        todo!()
    }

    async fn flush(&self) -> Result<(), Self::FlushError> {
        todo!()
    }

    async fn payload(&self, payload_digest: &PD) -> Option<FromSlice<u8>> {
        todo!()
    }

    async fn entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>> {
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

    async fn resume_subscription(
        &self,
        progress_id: u64,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Result<
        FromSlice<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
        willow_data_model::ResumptionFailedError,
    > {
        todo!()
    }
}
