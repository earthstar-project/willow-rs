use either::Either;
use std::{convert::Infallible, marker::PhantomData};

use sled::{Db, Error as SledError, IVec, Result as SledResult, Tree};
use ufotofu::{
    consumer::IntoVec, producer::FromSlice, BufferedConsumer, BufferedProducer, BulkConsumer,
    BulkProducer, Consumer, Producer,
};
use ufotofu_codec::{
    Decodable, DecodableSync, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use ufotofu_codec_endian::U64BE;
use willow_data_model::{
    AuthorisationToken, AuthorisedEntry, Component, Entry, EntryIngestionError,
    EntryIngestionSuccess, LengthyAuthorisedEntry, NamespaceId, Path, PayloadDigest, Store,
    StoreEvent, SubspaceId,
};

pub struct StoreSimpleSled {
    db: Db,
}

const ENTRY_TREE_KEY: [u8; 1] = [0b0000_0000];
const OPS_TREE_KEY: [u8; 1] = [0b0000_0001];
const PAYLOAD_TREE_KEY: [u8; 1] = [0b0000_0010];
const MISC_TREE_KEY: [u8; 1] = [0b0000_0011];

impl StoreSimpleSled {
    pub fn new<N>(namespace: N) -> Self
    where
        N: NamespaceId + EncodableKnownSize + EncodableSync,
    {
        todo!()
    }

    pub fn from_db(db: Db) -> Self {
        Self { db }
    }

    fn entry_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(ENTRY_TREE_KEY)
    }

    fn ops_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(OPS_TREE_KEY)
    }

    fn payload_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(PAYLOAD_TREE_KEY)
    }

    fn misc_tree(&self) -> SledResult<Tree> {
        self.db.open_tree(MISC_TREE_KEY)
    }

    /// Return whether this store contains entries with paths that are prefixes of the given path and newer than the given timestamp
    async fn is_prefixed_by_newer<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        PD,
        AT,
    >(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        timestamp: u64,
    ) -> Result<Option<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, SimpleStoreSledError>
    where
        N: NamespaceId + EncodableKnownSize + Decodable,
        S: SubspaceId + EncodableSync + EncodableKnownSize + Decodable,
        PD: PayloadDigest + Decodable,
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
                decode_entry_key::<MCL, MCC, MPL, S>(key).await;

            if timestamp < other_timestamp && path.is_prefixed_by(&other_path) {
                let (payload_length, payload_digest, authorisation_token, _operation_id) =
                    decode_entry_values(value).await;

                let entry = Entry::new(
                    Store::<MCL, MCC, MPL, N, S, PD, AT>::namespace_id(self).await?,
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
    Store<MCL, MCC, MPL, N, S, PD, AT> for StoreSimpleSled
where
    N: NamespaceId + Encodable + EncodableKnownSize + Decodable,
    S: SubspaceId + EncodableSync + EncodableKnownSize + Decodable,
    PD: PayloadDigest + Decodable,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Decodable,
    S::ErrorReason: core::fmt::Debug,
    PD::ErrorReason: core::fmt::Debug,
    AT::ErrorReason: core::fmt::Debug,
{
    type FlushError = SimpleStoreSledError;

    type BulkIngestionError = SimpleStoreSledError;

    type OperationsError = SimpleStoreSledError;

    async fn namespace_id(&self) -> Result<N, Self::OperationsError> {
        let tree = self.misc_tree()?;

        let namespace_bytes = tree
            .get(b"namespace_id")?
            .expect("There should be a namespace saved here...");

        let mut producer = FromSlice::new(&namespace_bytes);

        let namespace = N::decode(&mut producer)
            .await
            .map_err(|_| SimpleStoreSledError {})?;

        Ok(namespace)
    }

    async fn ingest_entry(
        &self,
        authorised_entry: willow_data_model::AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
    ) -> Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, Self::OperationsError>,
    > {
        let (entry, token) = authorised_entry.into_parts();

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

        // Check for existing entries with the same subspace + path
        //      If their timestamp is greater, no-op
        //      If their timestamp is same but digest is greater, no-op
        //      If their timestamp and digest is same but payload length is greater, no-op.

        // If prevent_pruning == true, check if this would prune and abort if so.

        // Insert and prune all entries with paths prefixed by this one with lesser timestamps.

        // Transform the entry into a simple sled key value pair.

        todo!()
    }

    async fn bulk_ingest_entry(
        &self,
        authorised_entries: &[willow_data_model::AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>],
        prevent_pruning: bool,
    ) -> Result<
        Vec<
            willow_data_model::BulkIngestionResult<
                MCL,
                MCC,
                MPL,
                N,
                S,
                PD,
                AT,
                Self::OperationsError,
            >,
        >,
        willow_data_model::BulkIngestionError<
            MCL,
            MCC,
            MPL,
            N,
            S,
            PD,
            AT,
            Self::BulkIngestionError,
            Self::OperationsError,
        >,
    > {
        todo!()
    }

    async fn append_payload<Producer>(
        &self,
        expected_digest: &PD,
        expected_size: u64,
        payload_source: &mut Producer,
    ) -> Result<
        willow_data_model::PayloadAppendSuccess<MCL, MCC, MPL, N, S, PD>,
        willow_data_model::PayloadAppendError<Self::OperationsError>,
    >
    where
        Producer: BulkProducer<Item = u8>,
    {
        todo!()
    }

    async fn forget_entry(
        &self,
        path: &willow_data_model::Path<MCL, MCC, MPL>,
        subspace_id: &S,
        traceless: bool,
    ) -> Result<(), Self::OperationsError> {
        todo!()
    }

    async fn forget_area(
        &self,
        area: &willow_data_model::grouping::AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<willow_data_model::grouping::Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> Vec<willow_data_model::AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }

    async fn forget_payload(
        path: &willow_data_model::Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> Result<(), willow_data_model::ForgetPayloadError> {
        todo!()
    }

    async fn force_forget_payload(
        path: &willow_data_model::Path<MCL, MCC, MPL>,
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

    async fn flush() -> Result<(), Self::FlushError> {
        todo!()
    }

    // FromSlice is placeholder so we can COMPILE
    async fn payload(&self, payload_digest: &PD) -> Option<FromSlice<u8>> {
        todo!()
    }

    async fn entry(
        &self,
        path: &willow_data_model::Path<MCL, MCC, MPL>,
        subspace_id: &S,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Option<willow_data_model::LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }

    fn query_area(
        &self,
        area: &willow_data_model::grouping::AreaOfInterest<MCL, MCC, MPL, S>,
        order: &willow_data_model::QueryOrder,
        reverse: bool,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
        // FromSlice is placeholder so we can COMPILE
    ) -> DummyProducer<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }

    fn subscribe_area(
        &self,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> DummyProducer<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>> {
        todo!()
    }

    async fn resume_subscription(
        &self,
        progress_id: u64,
        area: &willow_data_model::grouping::Area<MCL, MCC, MPL, S>,
        ignore: Option<willow_data_model::QueryIgnoreParams>,
    ) -> Result<
        DummyProducer<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
        willow_data_model::ResumptionFailedError,
    > {
        todo!()
    }
}

async fn encode_entry_key<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S: SubspaceId + EncodableKnownSize + EncodableSync,
>(
    subspace: S,
    path: Path<MCL, MCC, MPL>,
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
    encoded: IVec,
) -> (S, Path<MCL, MCC, MPL>, u64)
where
    S::ErrorReason: core::fmt::Debug,
{
    let mut producer = FromSlice::new(&encoded);

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
    payload_digest: PD,
    auth_token: AT,
    operation_id: u64,
) -> Vec<u8>
where
    PD: Encodable,
    AT: Encodable,
{
    let mut consumer: IntoVec<u8> = IntoVec::new();

    U64BE(payload_length).encode(&mut consumer).await.unwrap();
    payload_digest.encode(&mut consumer).await.unwrap();
    auth_token.encode(&mut consumer).await.unwrap();
    U64BE(operation_id).encode(&mut consumer).await.unwrap();

    consumer.into_vec()
}

async fn decode_entry_values<PD, AT>(encoded: IVec) -> (u64, PD, AT, u64)
where
    PD: Decodable,
    AT: Decodable,
    PD::ErrorReason: core::fmt::Debug,
    AT::ErrorReason: core::fmt::Debug,
{
    let mut producer = FromSlice::new(&encoded);

    let payload_length = U64BE::decode(&mut producer).await.unwrap().0;
    let payload_digest = PD::decode(&mut producer).await.unwrap();
    let auth_token = AT::decode(&mut producer).await.unwrap();
    let operation_id = U64BE::decode(&mut producer).await.unwrap().0;

    (payload_length, payload_digest, auth_token, operation_id)
}

impl From<SledError> for SimpleStoreSledError {
    fn from(_value: SledError) -> Self {
        SimpleStoreSledError {}
    }
}

struct DummyProducer<T> {
    thing: T,
}

impl<T> Producer for DummyProducer<T> {
    type Item = T;

    type Final = ();

    type Error = Infallible;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        todo!("You should not actually be *running* this DummyProducer, Dummy.")
    }
}
