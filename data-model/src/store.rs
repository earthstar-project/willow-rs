use std::{
    error::Error,
    fmt::{Debug, Display},
    future::Future,
};

use either::Either;
use ufotofu::{BulkConsumer, BulkProducer, Producer};

use crate::{
    entry::AuthorisedEntry,
    grouping::{Area, AreaSubspace, Range},
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

/// A notification about changes in a [`Store`]. You can obtain a producer of these via the [`Store::subscribe_area`] method.
///
/// An event subscription takes two parameters: the [`Area`] within events should be reported (any store mutations outside that area will not be reported to that subscription), and some optional `QueryIgnoreParams` for optionally filtering events based on whether they correspond to entries whose payload is the empty string and/or whose payload is not fully available in the local store. A more detailed description of how these ignore options impact events is given in the docs for each enum variant, but the general intuition is for the subscription to act as if it was on a store that did not inlcude ignored entries in the first place.
///
/// In the description of the enum variants, we write `sub_area` for the area of the subscription, and `ignores` for the subscription `QueryIgnoreParams`.
#[derive(Debug, Clone)]
pub enum StoreEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Emitted when an entry is inserted in `area`.
    ///
    /// - If `ignores.ignore_empty_payloads`, this is not emitted if the payload of the entry is the empty payload.
    /// - If `ignores.ignore_incomplete_payloads`, this event is not emitted upon entry insertion, but only once its payload has been fully added to the store. In this case, the ingestion event is guaranteed to be emitted *before* the corresponding payload append event.
    Ingested {
        /// The entry that was inserted.
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        /// A tag that determines whether we ourselves *created* this entry, or whether it arrived from some other data source. In the latter case, the data source is identified by a u64 id. This is not necessarily intented for application-dev-facing APIs, but rather for efficiently implementing replication services (where you want to forward new entries to other peers, but not to those from which you have just received them).
        origin: EntryOrigin,
    },
    /// Emitted whenever one or more entries are removed from `area` (via prefix pruning) because of an insertion that did not itself happen inside `area`. Example: `area.path` is `["blog", "recipes"]`, and a new entry is written to `[blog]`, thus deleting all old recipes.
    ///
    /// Of the "one or more entries", at least one must not have been ignored by the `ignores`. If all deleted entries are ignored, no corresponding `Pruned` event is emitted.
    ///
    /// Note that no such event is emitted when the insertion falls *into* `area`; subscribers must monitor `StoreEvent::Ingested` events and infer any deletions from those.
    Pruned {
        /// The path of the entry that caused the pruning.
        path: Path<MCL, MCC, MPL>,
        /// The subspace_id of the entry that caused the pruning.
        subspace_id: S,
        /// The timestamp of the entry that caused the pruning.
        timestamp: u64,
    },
    /// An existing entry inside `area` received a portion of its corresponding payload.
    ///
    /// If `ignores.ignore_incomplete_payloads`, this is only emitted when the payload is now fully available. In this case, the corresponding `Ingested` event is guaranteed to be emitted before this `Appended` event.
    Appended(LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// Emitted whenever a non-ignored entry in `area` is forgotten via `Store::forget_entry`. No corresponding `PayloadForgotten` event is emitted in this case.
    EntryForgotten {
        /// The path of the forgotten entry.
        path: Path<MCL, MCC, MPL>,
        /// The subspace_id of the forgotten entry.
        subspace_id: S,
        /// The timestamp of the forgotten entry.
        timestamp: u64,
        // TODO authorised entry instead? Also the payload maybe? Apps might need to retrieve the payload in order to "undo" operations encoded therein, but also the whole point is to get rid of the payload, not to store it for event consumers. Will probably keep things without the payload for now, but there might be a future API that enables payload access.
    },
    /// Emitted whenever a call to `Store::forget_area` forgets at least one non-ignored entry in `area`. No corresponding `AreaPayloadForgotten` event is emitted in this case.
    AreaForgotten {
        /// The area that was forgotten.
        area: Area<MCL, MCC, MPL, S>,
        /// A subarea that was retained (if any).
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    /// Emitted whenever the payload of a non-ignored entry in `area` is forgotten via `Store::forget_payload`. Emitted even if no payload bytes had been available to forget in the first place.
    PayloadForgotten(AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// Emitted whenever the payload of at least one non-ignored entry in `area` is forgotten via `Store::forget_area_payloads` Emitted even if no payload bytes had been available to forget in the first place.
    AreaPayloadsForgotten {
        /// The area whose payloads were forgotten.
        area: Area<MCL, MCC, MPL, S>,
        /// A subarea whose payloads were retained (if any).
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    StoreEvent<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    pub fn included_by_area(&self, area: &Area<MCL, MCC, MPL, S>) -> bool {
        match self {
            StoreEvent::Ingested { entry, origin: _ } => area.includes_entry(entry.entry()),
            StoreEvent::Appended(lengthy_authorised_entry) => {
                area.includes_entry(lengthy_authorised_entry.entry().entry())
            }
            StoreEvent::EntryForgotten {
                path,
                subspace_id,
                timestamp,
            } => {
                area.subspace().includes(subspace_id)
                    && area.path().is_prefix_of(path)
                    && area.times().includes(timestamp)
            }
            StoreEvent::PayloadForgotten(entry) => area.includes_entry(entry.entry()),
            StoreEvent::Pruned {
                subspace_id,
                path,
                timestamp,
            } => {
                // To be included by an area,
                // The originating entry must exist OUTSIDE the area
                // AND the area pruned by that entry must intersect with the given area
                !area.includes_triplet(subspace_id, path, *timestamp)
                    && Area::new(
                        AreaSubspace::Id(subspace_id.clone()),
                        path.clone(),
                        Range::new_closed(0, *timestamp).unwrap(),
                    )
                    .intersection(area)
                    .is_some()
            }
            StoreEvent::AreaForgotten {
                area: forgotten_area,
                protected: _,
            } => area.intersection(forgotten_area).is_some(),
            StoreEvent::AreaPayloadsForgotten {
                area: forgotten_area,
                protected: _,
            } => area.intersection(forgotten_area).is_some(),
        }
    }
}

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
pub enum ForgetPayloadError<OE: Debug> {
    NoSuchEntry,
    ReferredToByOtherEntries,
    OperationsError(OE),
}

impl<OE: Debug> Display for ForgetPayloadError<OE> {
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
            ForgetPayloadError::OperationsError(_) => {
                write!(f, "There store encountered an internal error.")
            }
        }
    }
}

impl<OE: Debug> Error for ForgetPayloadError<OE> {}

/// The origin of an entry ingestion event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOrigin {
    /// The entry was probably created on this machine.
    Local,
    /// The entry was sourced from another source with an ID assigned by us.
    /// This is useful if you want to suppress the forwarding of entries to the peers from which the entry was originally sourced.
    Remote(u64),
}

pub enum EventSenderError<OE> {
    /// The store threw an error.
    StoreError(OE),
    /// You failed to process events quickly enough.
    DoTryToKeepUp,
}

/// A [`Store`] is a set of [`AuthorisedEntry`] belonging to a single namespace, and a  (possibly partial) corresponding set of payloads.
pub trait Store<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    type Error: Display + Error;

    /// Returns the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) which all of this store's [`AuthorisedEntry`] belong to.
    fn namespace_id(&self) -> &N;

    /// Attempts to ingest an [`AuthorisedEntry`] into the [`Store`].
    ///
    /// Will fail if the entry belonged to a different namespace than the store's, or if the `prevent_pruning` param is `true` and an ingestion would have triggered [prefix pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning).
    fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        prevent_pruning: bool,
        origin: EntryOrigin,
    ) -> impl Future<
        Output = Result<
            EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
            EntryIngestionError<Self::Error>,
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
        origin: EntryOrigin,
    ) -> impl Future<Output = Result<(), BulkIngestionError<P::Error, C::Error>>>
    where
        P: BulkProducer<Item = AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
        C: BulkConsumer<
            Item = Result<
                EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
                EntryIngestionError<Self::Error>,
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
                        let result = self
                            .ingest_entry(authed_entry, prevent_pruning, origin.clone())
                            .await;

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
        Output = Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::Error>>,
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
    ) -> impl Future<Output = Result<(), Self::Error>>;

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
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, or an error if no entry with that path and subspace ID is held by this store or if the entry's payload corresponds to other entries.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Locally forgets all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning a count of forgotten payloads. Payloads corresponding to entries *outside* of the given `area` param will be be prevented from being forgotten.
    ///
    /// If `protected` is `Some`, then all payloads corresponding to entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Forces persistence of all previous mutations
    fn flush(&self) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns a [`ufotofu::Producer`] of bytes for the payload corresponding to the given subspace id and path.
    fn payload(
        &self,
        subspace: &S,
        path: &Path<MCL, MCC, MPL>,
    ) -> impl Future<Output = Result<Option<impl Producer<Item = u8>>, Self::Error>>;

    /// Returns a [`LengthyAuthorisedEntry`] with the given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) ID, if present.
    fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, Self::Error>,
    >;

    /// Queries which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Area`], returning a producer of [`LengthyAuthorisedEntry`] **produced in an arbitrary order decided by the store implementation**.
    fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        reverse: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,
            Self::Error,
        >,
    >;

    /// Subscribes to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = impl Producer<
            Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
            Error = EventSenderError<Self::Error>,
        >,
    >;
}
