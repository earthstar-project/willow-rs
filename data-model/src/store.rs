use std::future::Future;

use ufotofu::{local_nb::Producer, nb::BulkProducer};

use crate::{
    entry::AuthorisedEntry,
    grouping::AreaOfInterest,
    parameters::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId},
    LengthyAuthorisedEntry, LengthyEntry, Path,
};

/// Returned when an entry could be ingested into a [`Store`].
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
pub enum EntryIngestionError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT,
> {
    /// The entry belonged to another namespace.
    WrongNamespace(AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// The ingestion would have triggered prefix pruning when that was not desired.
    PruningPrevented,
}

/// A tuple of an [`AuthorisedEntry`] and how a [`Store`] responded to its ingestion.
pub type BulkIngestionResult<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> = (
    AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT>,
    >,
);

/// Returned when a bulk ingestion failed due to a consumer error.
pub struct BulkIngestionError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    IngestionError,
> {
    pub ingested: Vec<BulkIngestionResult<MCL, MCC, MPL, N, S, PD, AT>>,
    pub error: IngestionError,
}

/// Return when a payload is successfully appended to the [`Store`].
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
pub enum PayloadAppendError {
    /// None of the entries in the store reference this payload.
    NotEntryReference,
    /// The payload is already held in storage.
    AlreadyHaveIt,
    /// The received payload is larger than was expected.
    PayloadTooLarge,
    /// The completed payload's digest is not what was expected.
    DigestMismatch,
    /// Try deleting some files!!!
    SomethingElseWentWrong,
}

/// Returned when no entry was found for some criteria.
pub struct NoSuchEntryError;

/// Orderings for a
pub enum QueryOrder {
    /// Ordered by subspace, then path, then timestamp
    Subspace,
    /// Ordered by path, them timestamp, then subspace
    Path,
    /// Ordered by timestamp, then subspace, then path
    Timestamp,
    /// Whichever order is most efficient.
    Efficient,
}

/// Describes an [`AuthorisedEntry`] which was pruned and the [`AuthorisedEntry`] which triggered the pruning.
pub struct PruneEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// The entry which was pruned.
    pub pruned: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    /// The entry which triggered the pruning.
    pub by: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
}

/// An event which took place within a [`Store`].
/// Each event includes a *progress ID* which can be used to *resume* a subscription at any point in the future.
pub enum StoreEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// A new entry was ingested.
    Ingested(u64, AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// An existing entry received a portion of its corresponding payload.
    Appended(u64, LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// An entry was forgotten.
    EntryForgotten(u64, AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// A payload was forgotten.
    PayloadForgotten(u64, PD),
    /// An entry was pruned via prefix pruning.
    Pruned(u64, PruneEvent<MCL, MCC, MPL, N, S, PD, AT>),
}

/// Returned when the store chooses to not resume a subscription.
pub struct ResumptionFailedError(pub u64);

/// Describes which entries to ignore during a query.
#[derive(Default)]
pub struct QueryIgnoreParams {
    /// Omit entries with locally incomplete corresponding payloads.
    pub ignore_incomplete_payloads: bool,
    /// Omit entries with an empty payload.
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

/// A [`Store`] is a set of [`AuthorisedEntry`] belonging to a single namespace, and a  (possibly partial) corresponding set of payloads.
pub trait Store<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    type FlushError;
    type BulkIngestionError;

    /// The [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) which all of this store's [`AuthorisedEntry`] belong to.
    fn namespace_id() -> N;

    /// Attempt to ingest an [`AuthorisedEntry`] into the [`Store`].
    /// Will fail if the entry belonged to a different namespace than the store's, or if the `prevent_pruning` param is `true` and an ingestion would have triggered [prefix pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning).
    fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
    ) -> impl Future<
        Output = Result<
            EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
            EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT>,
        >,
    >;

    /// Attempt to ingest many [`AuthorisedEntry`] in the [`Store`].
    ///
    /// The result being `Ok` does **not** indicate that all entry ingestions were successful, only that each entry had an ingestion attempt, some of which *may* have returned [`EntryIngestionError`]. The `Err` type of this result is only returned if there was some internal error.
    fn bulk_ingest_entry(
        &self,
        authorised_entries: &[AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>],
        prevent_pruning: bool,
    ) -> impl Future<
        Output = Result<
            Vec<BulkIngestionResult<MCL, MCC, MPL, N, S, PD, AT>>,
            BulkIngestionError<MCL, MCC, MPL, N, S, PD, AT, Self::BulkIngestionError>,
        >,
    >;

    /// Attempt to append part of a payload for a given [`AuthorisedEntry`].
    ///
    /// Will fail if:
    /// - The payload digest is not referred to by any of the store's entries.
    /// - A complete payload with the same digest is already held in storage.
    /// - The payload exceeded the expected size
    /// - The final payload's digest did not match the expected digest
    /// - Something else went wrong, e.g. there was no space for the payload on disk.
    ///
    /// This method **cannot** verify the integrity of partial payload. This means that arbitrary (and possibly malicious) payloads smaller than the expected size will be stored unless partial verification is implemented upstream (e.g. during [the Willow General Sync Protocol's payload transformation](https://willowprotocol.org/specs/sync/index.html#sync_payloads_transform)).
    fn append_payload<Producer>(
        &self,
        expected_digest: &PD,
        expected_size: u64,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<PayloadAppendSuccess<MCL, MCC, MPL, N, S, PD>, PayloadAppendError>>
    where
        Producer: BulkProducer<Item = u8>;

    /// Locally forget an entry with a given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) id, returning the forgotten entry, or an error if no entry with that path and subspace ID are held by this store.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the entry. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        traceless: bool,
    ) -> impl Future<Output = Result<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, NoSuchEntryError>>;

    /// Locally forget all [`AuthorisedEntry`] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all forgotten entries
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten entries. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;

    /// Locally forget all [`AuthorisedEntry`] **not** [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all forgotten entries
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten entries. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_everything_but_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;

    /// Locally forget a payload with a given [`PayloadDigest`], or an error if no payload with that digest is held by this store.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the payload. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        &self,
        digest: PD,
        traceless: bool,
    ) -> impl Future<Output = Result<(), NoSuchEntryError>>;

    /// Locally forget all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all [`PayloadDigest`] of forgotten payloads.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten payloads. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_area_payloads(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<PD>>;

    /// Locally forget all payloads with corresponding [`AuthorisedEntry`] **not** [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all [`PayloadDigest`] of forgotten payloads.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten payloads. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_everything_but_area_payloads(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<PD>>;

    /// Force persistence of all previous mutations
    fn flush() -> impl Future<Output = Result<(), Self::FlushError>>;

    /// Return a [`LengthyAuthorisedEntry`] with the given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) ID, if present.
    ///
    /// If `ignore_incomplete_payloads` is `true`, will return `None` if the entry's corresponding payload  is incomplete, even if there is an entry present.
    /// If `ignore_empty_payloads` is `true`, will return `None` if the entry's payload length is `0`, even if there is an entry present.
    fn entry(
        &self,
        path: &Path<MCL, MCC, MPL>,
        subspace_id: &S,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<Output = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Query which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of [`LengthyAuthorisedEntry`].
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn query_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        order: &QueryOrder,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Subscribe to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn subscribe_area(
        &self,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Attempt to resume a subscription using a *progress ID* obtained from a previous subscription, or return an error if this store implementation is unable to resume the subscription.
    fn resume_subscription(
        &self,
        progress_id: u64,
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;
}
