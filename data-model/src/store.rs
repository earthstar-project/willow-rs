use std::{
    error::Error,
    fmt::{Debug, Display},
    future::Future,
};

use either::Either;
use ufotofu::{BulkConsumer, BulkProducer, Producer};

use crate::{
    entry::AuthorisedEntry,
    grouping::{Area, AreaOfInterest},
    parameters::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId},
    LengthyAuthorisedEntry, Path,
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
pub enum EntryIngestionError<OE: Display + Error> {
    /// The ingestion would have triggered prefix pruning when that was not desired.
    PruningPrevented,
    /// The entry's authorisation token is invalid.
    NotAuthorised,
    /// Something specific to this store implementation went wrong.
    OperationsError(OE),
}

impl<OE: Display + Error> Display for EntryIngestionError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryIngestionError::PruningPrevented => {
                write!(f, "Entry ingestion would have triggered undesired pruning.")
            }
            EntryIngestionError::NotAuthorised => {
                write!(f, "The entry had an invalid authorisation token")
            }
            EntryIngestionError::OperationsError(err) => Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for EntryIngestionError<OE> {}

/// Returned when a bulk ingestion failed due to a consumer error.
#[derive(Debug, Clone)]
pub enum BulkIngestionError<PE, CE> {
    Producer(PE),
    Consumer(CE),
}

impl<PE, OE> std::fmt::Display for BulkIngestionError<PE, OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BulkIngestionError::Producer(_) => {
                write!(f, "A producer error stopped bulk ingestion")
            }
            BulkIngestionError::Consumer(_) => {
                write!(f, "A consumer error stopped bulk ingestion")
            }
        }
    }
}

impl<PE: Display + Error, OE: Display + Error> Error for BulkIngestionError<PE, OE> {}

/// Returned when a payload is successfully appended to the [`Store`].
#[derive(Debug, Clone)]
pub enum PayloadAppendSuccess {
    /// The payload was appended to but not completed.
    Appended,
    /// The payload was completed by the appendment.
    Completed,
}

/// Returned when a payload fails to be appended into the [`Store`].
#[derive(Debug, Clone)]
pub enum PayloadAppendError<PayloadSourceError, OE> {
    /// The payload is already held in storage.
    AlreadyHaveIt,
    /// The payload source produced more bytes than were expected for this payload.
    TooManyBytes,
    /// The completed payload's digest is not what was expected.
    DigestMismatch,
    /// The source that provided the payload bytes emitted an error.
    SourceError {
        source_error: PayloadSourceError,
        /// Returns how many bytes of payload the store now stores for this entry.
        total_length_now_available: u64,
    },
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<PayloadSourceError: Display + Error, OE: Display + Error> Display
    for PayloadAppendError<PayloadSourceError, OE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
            PayloadAppendError::SourceError { source_error, .. } => {
                write!(f, "The payload source emitted an error: {}", source_error)
            }
            PayloadAppendError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<PayloadSourceError: Display + Error, OE: Display + Error> Error
    for PayloadAppendError<PayloadSourceError, OE>
{
}

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
#[derive(Debug, Clone)]
pub enum StoreEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// A new entry was ingested.
    Ingested(AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, EntryOrigin),
    /// An existing entry received a portion of its corresponding payload.
    Appended(LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// An entry was forgotten.
    EntryForgotten((S, Path<MCL, MCC, MPL>)),
    /// A payload was forgotten.
    PayloadForgotten(PD),
    /// An entry was pruned via prefix pruning.
    Pruned(PruneEvent<MCL, MCC, MPL, N, S, PD, AT>),
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
pub enum ForceForgetPayloadError<OE: Debug> {
    NoSuchEntry,
    OperationsError(OE),
}

impl<OE: Debug> Display for ForceForgetPayloadError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForceForgetPayloadError::NoSuchEntry => {
                write!(
                    f,
                    "No entry for the given criteria could be found in this store."
                )
            }
            ForceForgetPayloadError::OperationsError(_) => {
                write!(f, "There store encountered an internal error.")
            }
        }
    }
}

impl<OE: Debug> Error for ForceForgetPayloadError<OE> {}

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
    fn namespace_id(&self) -> &N;

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
            EntryIngestionError<Self::OperationsError>,
        >,
    >;

    /// Attempts to ingest many [`AuthorisedEntry`] produced by a given `BulkProducer` into the [`Store`].
    ///
    /// The result being `Ok` does **not** indicate that all entry ingestions were successful, only that each entry had an ingestion attempt, some of which *may* have errored. The `Err` type of this result is only returned if there was some internal error.
    fn bulk_ingest_entry<P, C>(
        &self,
        entry_producer: &mut P,
        result_consumer: &mut C,
        prevent_pruning: bool,
    ) -> impl Future<Output = Result<(), BulkIngestionError<P::Error, C::Error>>>
    where
        P: BulkProducer<Item = AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        C: BulkConsumer<
            Item = Result<
                EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
                EntryIngestionError<Self::OperationsError>,
            >,
        >,
    {
        async move {
            loop {
                let next = entry_producer
                    .produce()
                    .await
                    .map_err(BulkIngestionError::Producer)?;

                match next {
                    Either::Left(authed_entry) => {
                        let result = self.ingest_entry(authed_entry, prevent_pruning).await;

                        result_consumer
                            .consume(result)
                            .await
                            .map_err(BulkIngestionError::Consumer)?;
                    }
                    Either::Right(_) => break,
                }
            }

            Ok(())
        }
    }

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
    fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
        payload_source: &mut Producer,
    ) -> impl Future<
        Output = Result<
            PayloadAppendSuccess,
            PayloadAppendError<PayloadSourceError, Self::OperationsError>,
        >,
    >
    where
        Producer: BulkProducer<Item = u8, Error = PayloadSourceError>;

    /// Locally forgets an entry with a given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) id, returning the forgotten entry, or an error if no entry with that path and subspace ID are held by this store.
    ///
    /// If the entry in question is the last remaining reference in the store to a particular [`PayloadDigest`], that payload will be forgotten from the store (if present).
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> impl Future<Output = Result<(), Self::OperationsError>>;

    /// Locally forgets all [`AuthorisedEntry`] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning the number of forgotten entries.
    ///
    /// If forgetting many entries causes no there to be no remaining references to certain payload digests, those payloads will be removed (if present).
    ///
    /// If `protected` is `Some`, then all entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> impl Future<Output = Result<usize, Self::OperationsError>>;

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, or an error if no entry with that path and subspace ID is held by this store or if the entry's payload corresponds to other entries.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> impl Future<Output = Result<(), Self::OperationsError>>;

    /// Locally forgets all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning a count of forgotten payloads. Payloads corresponding to entries *outside* of the given `area` param will be be prevented from being forgotten.
    ///
    /// If `protected` is `Some`, then all payloads corresponding to entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> impl Future<Output = Result<usize, Self::OperationsError>>;

    /// Forces persistence of all previous mutations
    fn flush(&self) -> impl Future<Output = Result<(), Self::FlushError>>;

    /// Returns a [`ufotofu::Producer`] of bytes for the payload corresponding to the given subspace id and path.
    fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> impl Future<Output = Result<Option<impl Producer<Item = u8>>, Self::OperationsError>>;

    /// Returns a [`LengthyAuthorisedEntry`] with the given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) ID, if present.
    fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<
            Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
            Self::OperationsError,
        >,
    >;

    /// Queries which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Area`], returning a producer of [`LengthyAuthorisedEntry`].
    fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        order: &QueryOrder,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
            Self::OperationsError,
        >,
    >;

    /// Subscribes to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;
}
