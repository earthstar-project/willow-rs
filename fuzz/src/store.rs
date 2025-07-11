use std::{
    cell::{RefCell, RefMut},
    collections::{BTreeMap, HashSet},
    convert::Infallible,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use arbitrary::Arbitrary;
use either::Either::{self, Left, Right};
use ufotofu::{
    producer::{FromBoxedSlice, FromSlice},
    BulkProducer, Producer,
};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    AuthorisationToken, AuthorisedEntry, Entry, EntryIngestionError, EntryIngestionSuccess,
    EntryOrigin, EventSystem, ForgetEntryError, ForgetPayloadError, LengthyAuthorisedEntry,
    NamespaceId, Path, Payload, PayloadAppendError, PayloadAppendSuccess, PayloadDigest,
    PayloadError, QueryIgnoreParams, Store, StoreEvent, SubspaceId, Timestamp,
};

#[derive(Debug)]
pub struct ControlStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
    namespace: N,
    subspaces: RefCell<BTreeMap<S, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>>>,
    event_system: Rc<RefCell<EventSystem<MCL, MCC, MPL, N, S, PD, AT, Infallible>>>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    ControlStore<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    pub fn new(namespace: N, capacity: usize) -> Self {
        Self {
            namespace,
            subspaces: RefCell::new(BTreeMap::new()),
            event_system: Rc::new(RefCell::new(EventSystem::new(capacity))),
        }
    }

    pub async fn prune(&self, authorised_entry: &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>)
    where
        PD: PayloadDigest,
        N: NamespaceId,
    {
        self.prune_maybe(authorised_entry, false, false).await;
    }

    async fn prune_maybe(
        &self,
        authorised_entry: &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        do_insert_if_necessary: bool,
    ) -> bool
    where
        PD: PayloadDigest,
        N: NamespaceId,
    {
        let subspace_store =
            self.get_or_create_subspace_store(authorised_entry.entry().subspace_id());

        self.prune_maybe_with_subspace_store(
            authorised_entry,
            prevent_pruning,
            do_insert_if_necessary,
            subspace_store,
        )
        .await
    }

    async fn prune_maybe_with_subspace_store<'s>(
        &'s self,
        authorised_entry: &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        do_insert_if_necessary: bool,
        mut subspace_store: RefMut<'s, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>>,
    ) -> bool
    where
        PD: PayloadDigest,
        N: NamespaceId,
    {
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
            return false;
        } else {
            for path_to_prune in prune_these {
                subspace_store.deref_mut().entries.remove(&path_to_prune);
            }

            if do_insert_if_necessary {
                subspace_store.deref_mut().entries.insert(
                    authorised_entry.entry().path().clone(),
                    ControlEntry {
                        authorisation_token: authorised_entry.token().to_owned(),
                        payload: Vec::new(),
                        payload_digest: authorised_entry.entry().payload_digest().to_owned(),
                        payload_length: authorised_entry.entry().payload_length(),
                        timestamp: authorised_entry.entry().timestamp(),
                    },
                );
            }

            return true;
        }
    }

    pub(crate) fn debug_cancel_subscribers(&self) {
        self.event_system.borrow().cancel_all_subscriptions();
    }

    fn get_or_create_subspace_store<'s>(
        &'s self,
        subspace_id: &S,
    ) -> RefMut<'s, ControlSubspaceStore<MCL, MCC, MPL, PD, AT>> {
        let mut subspaces = self.subspaces.borrow_mut();

        if !subspaces.contains_key(subspace_id) {
            let _ = subspaces.insert(subspace_id.clone(), ControlSubspaceStore::new());
        }

        RefMut::map(subspaces, |subspaces| {
            subspaces.get_mut(subspace_id).unwrap()
        })
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
    /// [newer than relation](https://willowprotocol.org/specs/data-model/index.html#entry_newer)
    fn is_newer_than<const MCL: usize, const MCC: usize, const MPL: usize, N, S>(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool {
        entry.timestamp() < self.timestamp
            || (entry.timestamp() == self.timestamp
                && *entry.payload_digest() < self.payload_digest)
            || (entry.timestamp() == self.timestamp
                && *entry.payload_digest() == self.payload_digest
                && entry.payload_length() < self.payload_length)
    }
}

impl<PD, AT> ControlEntry<PD, AT>
where
    PD: PayloadDigest,
{
    fn to_authorised_entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S>(
        &self,
        namespace_id: N,
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
    ) -> AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
    where
        N: NamespaceId,
        S: SubspaceId,
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    {
        AuthorisedEntry::new(
            Entry::new(
                namespace_id,
                subspace_id,
                path,
                self.timestamp,
                self.payload_length,
                self.payload_digest.to_owned(),
            ),
            self.authorisation_token.to_owned(),
        )
        .unwrap()
    }

    fn to_lengthy_authorised_entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S>(
        &self,
        namespace_id: N,
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
    ) -> LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
    where
        N: NamespaceId,
        S: SubspaceId,
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    {
        LengthyAuthorisedEntry::new(
            self.to_authorised_entry(namespace_id, subspace_id, path),
            self.payload.len() as u64,
        )
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

        let subspace_store =
            self.get_or_create_subspace_store(authorised_entry.entry().subspace_id());

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

        if self
            .prune_maybe_with_subspace_store(
                &authorised_entry,
                prevent_pruning,
                true,
                subspace_store,
            )
            .await
        {
            self.event_system
                .borrow_mut()
                .ingested_entry(authorised_entry, origin);
            Ok(EntryIngestionSuccess::Success)
        } else {
            Err(EntryIngestionError::PruningPrevented)
        }
    }

    async fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
        payload_source: &mut Producer,
    ) -> Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::Error>>
    where
        Producer: ufotofu::BulkProducer<Item = u8, Error = PayloadSourceError>,
    {
        let mut subspace_store = self.get_or_create_subspace_store(subspace);

        match subspace_store.entries.get_mut(path) {
            None => Err(PayloadAppendError::NoSuchEntry),
            Some(entry) => {
                if let Some(expected) = expected_digest {
                    if entry.payload_digest != expected {
                        return Err(PayloadAppendError::WrongEntry);
                    }
                }

                let max_length = entry.payload_length;
                let initial_length = entry.payload.len() as u64;
                let mut current_length = initial_length;

                while current_length < max_length {
                    let result = payload_source.expose_items().await.map_err(|err| {
                        PayloadAppendError::SourceError {
                            source_error: err,
                            total_length_now_available: current_length,
                        }
                    })?;

                    let produced_count = match result {
                        Left(bytes) => {
                            current_length = current_length.saturating_add(bytes.len() as u64);
                            if current_length > max_length {
                                break;
                            }

                            entry.payload.extend_from_slice(bytes);
                            Some(bytes.len())
                        }
                        Right(_fin) => None,
                    };

                    match produced_count {
                        Some(count) => {
                            payload_source
                                .consider_produced(count)
                                .await
                                .map_err(|err| PayloadAppendError::SourceError {
                                    source_error: err,
                                    total_length_now_available: current_length,
                                })?
                        }
                        None => break,
                    };
                }

                if let Ok(Left(_)) = payload_source.expose_items().await {
                    return Err(PayloadAppendError::TooManyBytes);
                }

                debug_assert_eq!(current_length, entry.payload.len() as u64);

                match current_length.cmp(&max_length) {
                    std::cmp::Ordering::Greater => Err(PayloadAppendError::TooManyBytes),
                    std::cmp::Ordering::Equal => {
                        let mut hasher = PD::hasher();
                        PD::write(&mut hasher, &entry.payload);

                        if PD::finish(&hasher) != entry.payload_digest {
                            entry.payload = vec![];

                            self.event_system.borrow_mut().forgot_payload(
                                entry.to_lengthy_authorised_entry(
                                    self.namespace_id().to_owned(),
                                    subspace.to_owned(),
                                    path.to_owned(),
                                ),
                            );

                            Err(PayloadAppendError::DigestMismatch)
                        } else {
                            if current_length != initial_length {
                                self.event_system.borrow_mut().appended_payload(
                                    entry.to_authorised_entry(
                                        self.namespace_id().to_owned(),
                                        subspace.to_owned(),
                                        path.to_owned(),
                                    ),
                                    initial_length,
                                    current_length,
                                );
                            }
                            Ok(PayloadAppendSuccess::Completed)
                        }
                    }
                    std::cmp::Ordering::Less => {
                        if current_length != initial_length {
                            self.event_system.borrow_mut().appended_payload(
                                entry.to_authorised_entry(
                                    self.namespace_id().to_owned(),
                                    subspace.to_owned(),
                                    path.to_owned(),
                                ),
                                initial_length,
                                current_length,
                            );
                        }
                        Ok(PayloadAppendSuccess::Appended)
                    }
                }
            }
        }
    }

    async fn forget_entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> Result<(), ForgetEntryError<Self::Error>> {
        let mut subspace_store = self.get_or_create_subspace_store(subspace_id);

        let found = subspace_store.entries.get(path);

        match found {
            None => Ok(()),
            Some(entry) => {
                if let Some(expected) = expected_digest {
                    if entry.payload_digest != expected {
                        return Err(ForgetEntryError::WrongEntry);
                    }
                }

                {
                    self.event_system
                        .borrow_mut()
                        .forgot_entry(entry.to_lengthy_authorised_entry(
                            self.namespace_id().to_owned(),
                            subspace_id.to_owned(),
                            path.to_owned(),
                        ));
                }

                subspace_store.entries.remove(path);

                Ok(())
            }
        }
    }

    async fn forget_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
        let mut candidates = vec![];

        let mut count = 0;

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if let Some(entry) = subspace_store.entries.get(path) {
                        if !area.times().includes(&entry.timestamp) {
                            continue;
                        }

                        if path.is_prefixed_by(area.path()) {
                            candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                        }
                    }
                }
            }
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if let Some(entry) = subspace_store.entries.get(path) {
                            if !area.times().includes(&entry.timestamp) {
                                continue;
                            }

                            if path.is_prefixed_by(area.path()) {
                                candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                            }
                        }
                    }
                }
            }
        }

        for candidate in candidates {
            if let Some(prot) = protected {
                if prot.includes_triplet(&candidate.0, &candidate.1, candidate.2.timestamp) {
                    continue;
                }
            }

            self.forget_entry(&candidate.0, &candidate.1, None)
                .await
                .expect("cannot fail when expected_digest is None");
            count += 1;
        }

        if count > 0 {
            self.event_system
                .borrow_mut()
                .forgot_area(area.to_owned(), protected.cloned());
        }

        Ok(count)
    }

    async fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> Result<(), ForgetPayloadError<Self::Error>> {
        let mut subspace_store = self.get_or_create_subspace_store(subspace_id);
        match subspace_store.entries.get_mut(path) {
            None => return Err(ForgetPayloadError::NoSuchEntry),
            Some(entry) => {
                if let Some(expected) = expected_digest {
                    if entry.payload_digest != expected {
                        return Err(ForgetPayloadError::WrongEntry);
                    }
                }

                self.event_system
                    .borrow_mut()
                    .forgot_payload(entry.to_lengthy_authorised_entry(
                        self.namespace_id().to_owned(),
                        subspace_id.to_owned(),
                        path.to_owned(),
                    ));

                entry.payload.clear();
            }
        }

        Ok(())
    }

    async fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> Result<usize, Self::Error> {
        let mut candidates = vec![];

        let mut count = 0;

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if let Some(entry) = subspace_store.entries.get(path) {
                        if !area.times().includes(&entry.timestamp) {
                            continue;
                        }

                        if path.is_prefixed_by(area.path()) {
                            candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                        }
                    }
                }
            }
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if let Some(entry) = subspace_store.entries.get(path) {
                            if !area.times().includes(&entry.timestamp) {
                                continue;
                            }

                            if path.is_prefixed_by(area.path()) {
                                candidates.push((subspace_id.clone(), path.clone(), entry.clone()));
                            }
                        }
                    }
                }
            }
        }

        for candidate in candidates {
            if let Some(prot) = protected {
                if prot.includes_triplet(&candidate.0, &candidate.1, candidate.2.timestamp) {
                    continue;
                }
            }

            self.forget_payload(&candidate.0, &candidate.1, None)
                .await
                .expect("cannot error if expectedDigest is None");
            count += 1;
        }

        if count > 0 {
            self.event_system
                .borrow_mut()
                .forgot_area_payloads(area.to_owned(), protected.cloned());
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
        offset: u64,
        expected_digest: Option<PD>,
    ) -> Result<
        Payload<Self::Error, impl BulkProducer<Item = u8, Final = (), Error = Self::Error>>,
        PayloadError<Self::Error>,
    > {
        let mut subspace_store = self.get_or_create_subspace_store(subspace);
        match subspace_store.entries.get_mut(path) {
            None => Err(PayloadError::NoSuchEntry),
            Some(entry) => {
                if let Some(expected) = expected_digest {
                    if entry.payload_digest != expected {
                        return Err(PayloadError::WrongEntry);
                    }
                }

                if offset >= (entry.payload.len() as u64) {
                    return Err(PayloadError::OutOfBounds);
                } else {
                    if entry.payload_length == entry.payload.len() as u64 {
                        return Ok(Payload::Complete(FromBoxedSlice::from_vec(
                            entry.payload[(offset as usize)..].to_vec(),
                        )));
                    } else {
                        return Ok(Payload::Incomplete(FromBoxedSlice::from_vec(
                            entry.payload[(offset as usize)..].to_vec(),
                        )));
                    }
                }
            }
        }
    }

    async fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: QueryIgnoreParams,
    ) -> Result<Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, Self::Error> {
        let subspace_store = self.get_or_create_subspace_store(subspace_id);
        match subspace_store.entries.get(path) {
            None => Ok(None),
            Some(entry) => {
                let available = entry.payload.len() as u64;

                if (ignore.ignore_empty_payloads && entry.payload_length == 0)
                    || (ignore.ignore_incomplete_payloads && available != entry.payload_length)
                {
                    return Ok(None);
                }

                Ok(Some(LengthyAuthorisedEntry::new(
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
                )))
            }
        }
    }

    async fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> Result<
        impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, Final = ()>,
        Self::Error,
    > {
        let mut candidates = vec![];

        match area.subspace() {
            AreaSubspace::Id(subspace_id) => {
                let subspace_store = self.get_or_create_subspace_store(subspace_id);

                for path in subspace_store.entries.keys() {
                    if !path.is_prefixed_by(area.path()) {
                        continue;
                    }

                    if let Some(entry) = subspace_store.entries.get(path) {
                        if !area.times().includes(&entry.timestamp) {
                            continue;
                        }

                        let available = entry.payload.len() as u64;

                        if (ignore.ignore_empty_payloads && entry.payload_length == 0)
                            || (ignore.ignore_incomplete_payloads
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
            AreaSubspace::Any => {
                for (subspace_id, subspace_store) in self.subspaces.borrow().iter() {
                    for path in subspace_store.entries.keys() {
                        if !path.is_prefixed_by(area.path()) {
                            continue;
                        }

                        if let Some(entry) = subspace_store.entries.get(path) {
                            if !area.times().includes(&entry.timestamp) {
                                continue;
                            }

                            let available = entry.payload.len() as u64;

                            if (ignore.ignore_empty_payloads && entry.payload_length == 0)
                                || (ignore.ignore_incomplete_payloads
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

        Ok(FromBoxedSlice::from_vec(candidates))
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

#[derive(Arbitrary, Debug)]
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
    AppendPayload {
        subspace: S,
        path: Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
        data: Vec<u8>,
    },
    ForgetEntry {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    },
    ForgetArea {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    ForgetPayload {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    },
    ForgetAreaPayloads {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    GetPayload {
        subspace: S,
        path: Path<MCL, MCC, MPL>,
        offset: u64,
        expected_digest: Option<PD>,
    },
    GetEntry {
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
        ignore: QueryIgnoreParams,
    },
    QueryArea {
        area: Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
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
    N: NamespaceId + std::hash::Hash,
    S: SubspaceId + std::hash::Hash,
    PD: PayloadDigest + std::hash::Hash,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>
        + Clone
        + std::fmt::Debug
        + PartialEq
        + Eq
        + std::hash::Hash,
    Store1: Store<MCL, MCC, MPL, N, S, PD, AT>,
    Store2: Store<MCL, MCC, MPL, N, S, PD, AT>,
    Store1::Error: std::fmt::Debug,
    Store2::Error: std::fmt::Debug,
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
                    match (store1.ingest_entry(authorised_entry.clone(), *prevent_pruning, *origin).await, store2.ingest_entry(authorised_entry.clone(), *prevent_pruning, *origin).await) {
                        (Ok(yay1), Ok(yay2)) => assert_eq!(yay1, yay2),
                        (Err(EntryIngestionError::PruningPrevented), Err(EntryIngestionError::PruningPrevented))   => continue,
                        (Err(EntryIngestionError::OperationsError(_)), Err(EntryIngestionError::OperationsError(_))) => {
                            panic!("AppendPayload: Producer failed, which indicates the two stores failed in different ways at the same time.")
                        }
                        (res1, res2) => panic!("IngestEntry: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}", res1, res2),
                    }
                }
            }
            StoreOp::AppendPayload {
                subspace,
                path,
                expected_digest,
                data,
            } => {
                let mut payload_1 = FromSlice::new(data);
                let mut payload_2 = FromSlice::new(data);

                let res_1 = store1
                    .append_payload(subspace, path, expected_digest.clone(), &mut payload_1)
                    .await;
                let res_2 = store2
                    .append_payload(subspace, path, expected_digest.clone(), &mut payload_2)
                    .await;

                match (res_1, res_2) {
                    (Ok(success_1), Ok(success_2)) => assert_eq!(success_1, success_2),
                    (Err(PayloadAppendError::NoSuchEntry), Err(PayloadAppendError::NoSuchEntry)) |(Err(PayloadAppendError::DigestMismatch), Err(PayloadAppendError::DigestMismatch)) | (Err(PayloadAppendError::TooManyBytes), Err(PayloadAppendError::TooManyBytes)) | (Err(PayloadAppendError::WrongEntry), Err(PayloadAppendError::WrongEntry)) => continue,
                    (Err(PayloadAppendError::OperationError(_)), Err(PayloadAppendError::OperationError(_))) => panic!("AppendPayload: Producer failed, which indicates the two stores failed in different ways at the same time."),
                    (res1, res2) => panic!("AppendPayload: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}", res1, res2),
                }
            }
            StoreOp::ForgetEntry {
                subspace_id,
                path,
                expected_digest,
            } => {
                match (
                    store1
                        .forget_entry(subspace_id, path, expected_digest.clone())
                        .await,
                    store2
                        .forget_entry(subspace_id, path, expected_digest.clone())
                        .await,
                ) {
                    (Ok(()), Ok(())) => {}
                    (Err(ForgetEntryError::WrongEntry), Err(ForgetEntryError::WrongEntry)) => continue,
                    (Err(ForgetEntryError::OperationError(_)), Err(ForgetEntryError::OperationError(_))) =>   panic!("ForgetEntry: Two stores happened to fail at the same time in different ways."),
                    (res1, res2) => panic!(
                        "ForgetEntry: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}",
                        res1, res2
                    ),
                }
            }
            StoreOp::ForgetArea { area, protected } => {
                match (
                    store1.forget_area(area, protected.as_ref()).await,
                    store2.forget_area(area, protected.as_ref()).await,
                ) {
                    (Ok(forgotten1), Ok(forgotten2)) => assert_eq!(forgotten1, forgotten2),
                    (Err(_), Err(_)) => {
                        panic!("ForgetArea: Two stores happened to fail at the same time in different ways.")
                    }
                    (res1, res2) => panic!(
                        "ForgetArea: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}",
                        res1, res2
                    ),
                }
            }
            StoreOp::ForgetPayload {
                subspace_id,
                path,
                expected_digest,
            } => {
                match (
                    store1
                        .forget_payload(subspace_id, path, expected_digest.clone())
                        .await,
                    store2
                        .forget_payload(subspace_id, path, expected_digest.clone())
                        .await,
                ) {
                    (Ok(()), Ok(())) => {}
                    (Err(ForgetPayloadError::WrongEntry), Err(ForgetPayloadError::WrongEntry)) | (Err(ForgetPayloadError::NoSuchEntry), Err(ForgetPayloadError::NoSuchEntry)) => continue,
                    (Err(ForgetPayloadError::OperationError(_)), Err(ForgetPayloadError::OperationError(_))) => {
                        panic!("ForgetPayload: Two stores happened to fail at the same time in different ways.")
                    }
                    (res1, res2) => panic!(
                        "ForgetPayload: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}",
                        res1, res2
                    ),
                }
            }
            StoreOp::ForgetAreaPayloads { area, protected } => {
                match (
                    store1.forget_area_payloads(area, protected.as_ref()).await,
                    store2.forget_area_payloads(area, protected.as_ref()).await,
                ) {
                    (Ok(forgotten1), Ok(forgotten2)) => assert_eq!(forgotten1, forgotten2),
                 (res1, res2) => panic!(
                     "ForgetAreaPayloads: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}",
                     res1, res2
                 ),
                }
            }
            StoreOp::GetPayload {
                subspace,
                path,
                offset,
                expected_digest,
            } => {
                match (
                    store1
                        .payload(subspace, path, *offset, expected_digest.clone())
                        .await,
                    store2
                        .payload(subspace, path, *offset, expected_digest.clone())
                        .await,
                ) {
                    (Ok(Payload::Complete(mut producer1)), Ok(Payload::Complete(mut producer2))) | (Ok(Payload::Incomplete(mut producer1)), Ok(Payload::Incomplete(mut producer2))) => loop {
                        match (producer1.produce().await, producer2.produce().await) {
                            (Ok(Either::Left(item1)), Ok(Either::Left(item2))) => {
                                assert_eq!(item1, item2)
                            }
                            (Ok(Either::Right(())), Ok(Either::Right(()))) => {
                                break;
                            }
                            (Err(_), Err(_)) => {
                                panic!("GetEntry: Two stores happened to fail at the same time in different ways.")
                            }
                            (_, _) => {
                                panic!("QueryArea: non-equivalent producer behaviour.")
                            }
                        }
                    },
                    (Err(PayloadError::WrongEntry), Err(PayloadError::WrongEntry)) | (Err(PayloadError::OutOfBounds), Err(PayloadError::OutOfBounds)) => continue,
                    (Err(PayloadError::OperationError(_)), Err(PayloadError::OperationError(_))) =>  panic!("Get Payload: Two stores happened to fail at the same time in different ways."),
                    // These are failing variants, but we don't use them because we can't print all of them
                    // (in particular, the bulk producers that could appear here don't have Debug on them)
                    (_, _) => panic!(
                        "GetPayload: non-equivalent behaviour.",
                    ),
                }
            }
            StoreOp::GetEntry {
                subspace_id,
                path,
                ignore,
            } => {
                match (
                    store1.entry(subspace_id, path, ignore.to_owned()).await,
                    store2.entry(subspace_id, path, ignore.to_owned()).await,
                ) {
                    (Ok(entry1), Ok(entry2)) => assert_eq!(entry1, entry2),
                    (res1, res2) => panic!(
                        "GetEntry: non-equivalent behaviour.\n\nStore 1: {:?}\n\nStore 2: {:?}",
                        res1, res2
                    ),
                }
            }
            StoreOp::QueryArea { area, ignore } => {
                match (
                    store1.query_area(area, ignore.to_owned()).await,
                    store2.query_area(area, ignore.to_owned()).await,
                ) {
                    (Ok(mut producer1), Ok(mut producer2)) => {
                        let mut set1 =
                            HashSet::<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>::new();
                        let mut set2 =
                            HashSet::<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>::new();

                        loop {
                            match producer1.produce().await {
                                Ok(Either::Left(entry)) => {
                                    set1.insert(entry);
                                }
                                Ok(Either::Right(_)) => break,
                                Err(_) => panic!("QueryArea: Store producer error"),
                            }
                        }

                        loop {
                            match producer2.produce().await {
                                Ok(Either::Left(entry)) => {
                                    set2.insert(entry);
                                }
                                Ok(Either::Right(_)) => break,
                                Err(_) => panic!("QueryArea: Store producer error"),
                            }
                        }

                        assert_eq!(set1, set2)
                    }
                    (_, _) => panic!("QueryArea: non-equivalent behaviour.",),
                }
            }
        }
    }
}

struct WrappedIngestResult<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, E>(
    Result<EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>, EntryIngestionError<E>>,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, E> Default
    for WrappedIngestResult<MCL, MCC, MPL, N, S, PD, AT, E>
{
    fn default() -> Self {
        Self(Ok(EntryIngestionSuccess::Success))
    }
}
