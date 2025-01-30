use std::{
    error::Error,
    fmt::{Debug, Display},
    future::Future,
};

use ufotofu::{local_nb::Producer, nb::BulkProducer};

use crate::{
    entry::AuthorisedEntry,
    grouping::{Area, AreaOfInterest},
    parameters::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId},
    LengthyAuthorisedEntry, LengthyEntry, Path,
};

/// Returned when an entry could be ingested into a [`Store`].
#[derive(Debug, Clone)]
pub enum EntryIngestionSuccess<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
> {
    /// The entry was successfully ingested.
    Success,
    /// The entry was not ingested because a newer entry with same
    Obsolete {
        /// The obsolete entry which was not ingested.
        obsolete: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        /// The newer entry which was not overwritten.
        newer: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
}

/// Returned when an entry cannot be ingested into a [`Store`].
#[derive(Debug, Clone)]
pub enum EntryIngestionError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT,
    OE: Display + Error,
> {
    /// The entry belonged to another namespace.
    WrongNamespace(AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// The ingestion would have triggered prefix pruning when that was not desired.
    PruningPrevented,
    /// Something specific to this store implementation went wrong.
    OperationsError(OE),
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        S: SubspaceId,
        PD: PayloadDigest,
        AT,
        OE: Display + Error,
    > Display for EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, OE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryIngestionError::WrongNamespace(_) => {
                write!(f, "Tried to ingest an entry from a different namespace.")
            }
            EntryIngestionError::PruningPrevented => {
                write!(f, "Entry ingestion would have triggered undesired pruning.")
            }
            EntryIngestionError::OperationsError(err) => Display::fmt(err, f),
        }
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Debug,
        S: SubspaceId + Debug,
        PD: PayloadDigest + Debug,
        AT: Debug,
        OE: Display + Error,
    > Error for EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, OE>
{
}

/// A tuple of an [`AuthorisedEntry`] and how a [`Store`] responded to its ingestion.
pub type BulkIngestionResult<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
    OE,
> = (
    AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, OE>,
    >,
);

/// Returned when a bulk ingestion failed due to a consumer error.
#[derive(Debug, Clone)]
pub struct BulkIngestionError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    OE: Error,
    IngestionError,
> {
    pub ingested: Vec<BulkIngestionResult<MCL, MCC, MPL, N, S, PD, AT, OE>>,
    pub error: IngestionError,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        S: SubspaceId,
        PD: PayloadDigest,
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
        OE: Display + Error,
        IngestionError: Error,
    > std::fmt::Display for BulkIngestionError<MCL, MCC, MPL, N, S, PD, AT, OE, IngestionError>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "An error stopped bulk ingestion after successfully ingesting {:?} entries",
            self.ingested.len()
        )
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Debug,
        S: SubspaceId + Debug,
        PD: PayloadDigest + Debug,
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Debug,
        OE: Display + Error,
        IngestionError: Error,
    > Error for BulkIngestionError<MCL, MCC, MPL, N, S, PD, AT, OE, IngestionError>
{
}

/// Returned when a payload is successfully appended to the [`Store`].
#[derive(Debug, Clone)]
pub enum PayloadAppendSuccess<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// The payload was appended to but not completed.
    Appended(Vec<LengthyEntry<MCL, MCC, MPL, N, S, PD>>),
    /// The payload was completed by the appendment.
    Completed(Vec<LengthyEntry<MCL, MCC, MPL, N, S, PD>>),
}

/// Returned when a payload fails to be appended into the [`Store`].
#[derive(Debug, Clone)]
pub enum PayloadAppendError<OE> {
    /// None of the entries in the store reference this payload.
    NotEntryReference,
    /// The payload is already held in storage.
    AlreadyHaveIt,
    /// The payload source produced more bytes than were expected for this payload.
    TooManyBytes,
    /// The completed payload's digest is not what was expected.
    DigestMismatch,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for PayloadAppendError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadAppendError::NotEntryReference => write!(
                f,
                "None of the entries in the soter reference this payload."
            ),
            PayloadAppendError::AlreadyHaveIt => {
                write!(f, "The payload is already held in storage.")
            }
            PayloadAppendError::TooManyBytes => write!(
                f,
                "The payload source produced more bytes than were expected for this payload."
            ),
            PayloadAppendError::DigestMismatch => {
                write!(f, "The complete payload's digest is not what was expected.")
            }
            PayloadAppendError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for PayloadAppendError<OE> {}

/// Returned when no entry was found for some criteria.
#[derive(Debug, Clone)]
pub struct NoSuchEntryError;

impl Display for NoSuchEntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No entry was found for the given criteria.")
    }
}

impl Error for NoSuchEntryError {}

/// The order by which entries should be returned for a given query.
#[derive(Debug, Clone)]
pub enum QueryOrder {
    /// Ordered by subspace, then path, then timestamp.
    Subspace,
    /// Ordered by path, then by an arbitrary order determined by the implementation.
    Path,
    /// Ordered by timestamp, then by an arbitrary order determined by the implementation.
    Timestamp,
    /// An arbitrary order chosen by the implementation, hopefully the most efficient one.
    Arbitrary,
}

/// Describes an [`AuthorisedEntry`] which was pruned and the [`AuthorisedEntry`] which triggered the pruning.
#[derive(Debug, Clone)]
pub struct PruneEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// The subspace ID and path of the entry which was pruned.
    pub pruned: (S, Path<MCL, MCC, MPL>),
    /// The entry which triggered the pruning.
    pub by: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
}

/// An event which took place within a [`Store`].
/// Each event includes a *progress ID* which can be used to *resume* a subscription at any point in the future.
#[derive(Debug, Clone)]
pub enum StoreEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// A new entry was ingested.
    Ingested(
        u64,
        AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        EntryOrigin,
    ),
    /// An existing entry received a portion of its corresponding payload.
    Appended(u64, LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// An entry was forgotten.
    EntryForgotten(u64, (S, Path<MCL, MCC, MPL>)),
    /// A payload was forgotten.
    PayloadForgotten(u64, PD),
    /// An entry was pruned via prefix pruning.
    Pruned(u64, PruneEvent<MCL, MCC, MPL, N, S, PD, AT>),
}

/// Returned when the store chooses to not resume a subscription.
#[derive(Debug, Clone)]
pub struct ResumptionFailedError(pub u64);

impl Display for ResumptionFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The subscription with ID {:?} could not be resumed.",
            self.0
        )
    }
}

impl Error for ResumptionFailedError {}

/// Describes which entries to ignore during a query.
#[derive(Default, Clone)]
pub struct QueryIgnoreParams {
    /// Omit entries with locally incomplete corresponding payloads.
    pub ignore_incomplete_payloads: bool,
    /// Omit entries whose payload is the empty string.
    pub ignore_empty_payloads: bool,
}

impl QueryIgnoreParams {
    pub fn ignore_incomplete_payloads(&mut self) {
        self.ignore_incomplete_payloads = true;
    }

    pub fn ignore_empty_payloads(&mut self) {
        self.ignore_empty_payloads = true;
    }
}

/// Returned when a payload could not be forgotten.
#[derive(Debug, Clone)]
pub enum ForgetPayloadError {
    NoSuchEntry,
    ReferredToByOtherEntries,
}

impl Display for ForgetPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForgetPayloadError::NoSuchEntry => {
                write!(
                    f,
                    "No entry for the given criteria could be found in this store."
                )
            }
            ForgetPayloadError::ReferredToByOtherEntries => write!(
                f,
                "The payload could not be forgotten because it is referred to by other entries."
            ),
        }
    }
}

impl Error for ForgetPayloadError {}

/// The origin of an entry ingestion event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOrigin {
    /// The entry was probably created on this machine.
    Local,
    /// The entry was sourced from another source with an ID assigned by us.
    /// This is useful if you want to suppress the forwarding of entries to the peers from which the entry was originally sourced.
    Remote(u64),
}

/// A [`Store`] is a set of [`AuthorisedEntry`] belonging to a single namespace, and a  (possibly partial) corresponding set of payloads.
pub trait Store<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    type FlushError: Display + Error;
    type BulkIngestionError: Display + Error;
    type OperationsError: Display + Error;

    /// Returns the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) which all of this store's [`AuthorisedEntry`] belong to.
    fn namespace_id() -> N;

    /// Attempts to ingest an [`AuthorisedEntry`] into the [`Store`].
    ///
    /// Will fail if the entry belonged to a different namespace than the store's, or if the `prevent_pruning` param is `true` and an ingestion would have triggered [prefix pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning).
    fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
    ) -> impl Future<
        Output = Result<
            EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
            EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT, Self::OperationsError>,
        >,
    >;

    /// Attempts to ingest many [`AuthorisedEntry`] in the [`Store`].
    ///
    /// The result being `Ok` does **not** indicate that all entry ingestions were successful, only that each entry had an ingestion attempt, some of which *may* have returned [`EntryIngestionError`]. The `Err` type of this result is only returned if there was some internal error.
    fn bulk_ingest_entry(
        &self,
        authorised_entries: &[AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>],
        prevent_pruning: bool,
    ) -> impl Future<
        Output = Result<
            Vec<BulkIngestionResult<MCL, MCC, MPL, N, S, PD, AT, Self::OperationsError>>,
            BulkIngestionError<
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
        >,
    >;

    /// Attempts to append part of a payload for a given [`AuthorisedEntry`].
    ///
    /// Will fail if:
    /// - The payload digest is not referred to by any of the store's entries.
    /// - A complete payload with the same digest is already held in storage.
    /// - The payload source produced more bytes than were expected for this payload.
    /// - The final payload's digest did not match the expected digest
    /// - Something else went wrong, e.g. there was no space for the payload on disk.
    ///
    /// This method **cannot** verify the integrity of partial payloads. This means that arbitrary (and possibly malicious) payloads smaller than the expected size will be stored unless partial verification is implemented upstream (e.g. during [the Willow General Sync Protocol's payload transformation](https://willowprotocol.org/specs/sync/index.html#sync_payloads_transform)).
    fn append_payload<Producer>(
        &self,
        expected_digest: &PD,
        expected_size: u64,
        payload_source: &mut Producer,
    ) -> impl Future<
        Output = Result<
            PayloadAppendSuccess<MCL, MCC, MPL, N, S, PD>,
            PayloadAppendError<Self::OperationsError>,
        >,
    >
    where
        Producer: BulkProducer<Item = u8>;

    /// Locally forgets an entry with a given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) id, returning the forgotten entry, or an error if no entry with that path and subspace ID are held by this store.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the entry. If `false`, it *may* persist what was forgotten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        traceless: bool,
    ) -> impl Future<Output = Result<(), Self::OperationsError>>;

    /// Locally forgets all [`AuthorisedEntry`] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all forgotten entries
    ///
    /// If `protected` is `Some`, then all entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten entries. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> impl Future<Output = Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, or an error if no entry with that path and subspace ID is held by this store or if the entry's payload corresponds to other entries.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the payload. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        path: &Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> impl Future<Output = Result<(), ForgetPayloadError>>;

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, or an error if no entry with that path and subspace ID is held by this store. **The payload will be forgotten even if it corresponds to other entries**.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the payload. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn force_forget_payload(
        path: &Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> impl Future<Output = Result<(), NoSuchEntryError>>;

    /// Locally forgets all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all [`PayloadDigest`] of forgotten payloads. Payloads corresponding to entries *outside* of the given `area` param will be be prevented from being forgotten.
    ///
    /// If `protected` is `Some`, then all payloads corresponding to entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten payloads. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_area_payloads(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> impl Future<Output = Vec<PD>>;

    /// Locally forgets all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all [`PayloadDigest`] of forgotten payloads. **Payloads will be forgotten even if it corresponds to other entries outside the given area**.
    ///
    /// If `protected` is `Some`, then all payloads corresponding to entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten payloads. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn force_forget_area_payloads(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
        traceless: bool,
    ) -> impl Future<Output = Vec<PD>>;

    /// Forces persistence of all previous mutations
    fn flush() -> impl Future<Output = Result<(), Self::FlushError>>;

    /// Returns a [`ufotofu::Producer`] of bytes for the payload corresponding to the given [`PayloadDigest`], if held.
    fn payload(
        &self,
        payload_digest: &PD,
    ) -> impl Future<Output = Option<impl Producer<Item = u8>>>;

    /// Returns a [`LengthyAuthorisedEntry`] with the given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) ID, if present.
    fn entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<Output = Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;

    /// Queries which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of [`LengthyAuthorisedEntry`].
    fn query_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        order: &QueryOrder,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Subscribes to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Attempts to resume a subscription using a *progress ID* obtained from a previous subscription, or return an error if this store implementation is unable to resume the subscription.
    fn resume_subscription(
        &self,
        progress_id: u64,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
            ResumptionFailedError,
        >,
    >;
}
