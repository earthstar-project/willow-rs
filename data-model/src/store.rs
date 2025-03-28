use std::{
    error::Error,
    fmt::{Debug, Display},
    future::Future,
};

use ufotofu::{BulkProducer, Producer};

use crate::{entry::AuthorisedEntry, grouping::Area, LengthyAuthorisedEntry, Path};

/// Returned when an entry could be ingested into a [`Store`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryIngestionSuccess<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryIngestionError<OE> {
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

/// Returned when a payload is successfully appended to the [`Store`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum PayloadAppendSuccess {
    /// The payload was appended to but not completed.
    Appended,
    /// The payload was completed by the appendment.
    Completed,
}

/// Returned when a payload fails to be appended into the [`Store`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum PayloadAppendError<PayloadSourceError, OE> {
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
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
            PayloadAppendError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to append to had an unexpected payload_digest."
                )
            }
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

/// Returned when forgetting an entry fails.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum ForgetEntryError<OE> {
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for ForgetEntryError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForgetEntryError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to append to had an unexpected payload_digest."
                )
            }
            ForgetEntryError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for ForgetEntryError<OE> {}

/// Returned when forgetting a payload fails.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum ForgetPayloadError<OE> {
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for ForgetPayloadError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForgetPayloadError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to append to had an unexpected payload_digest."
                )
            }
            ForgetPayloadError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for ForgetPayloadError<OE> {}

/// Returned when retrieving a payload fails.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum PayloadError<OE> {
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for PayloadError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to append to had an unexpected payload_digest."
                )
            }
            PayloadError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for PayloadError<OE> {}

/// A notification about changes in a [`Store`]. You can obtain a producer of these via the [`Store::subscribe_area`] method.
///
/// An event subscription takes two parameters: the [`Area`] within events should be reported (any store mutations outside that area will not be reported to that subscription), and some optional `QueryIgnoreParams` for optionally filtering events based on whether they correspond to entries whose payload is the empty string and/or whose payload is not fully available in the local store. A more detailed description of how these ignore options impact events is given in the docs for each enum variant, but the general intuition is for the subscription to act as if it was on a store that did not inlcude ignored entries in the first place.
///
/// In the description of the enum variants, we write `sub_area` for the area of the subscription, and `ignores` for the subscription `QueryIgnoreParams`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StoreEvent<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
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
        /// The entry that caused the pruning.
        cause: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
    /// An existing entry inside `area` received a portion of its corresponding payload.
    ///
    /// If `ignores.ignore_incomplete_payloads`, this is only emitted when the payload is now fully available. In this case, the corresponding `Ingested` event is guaranteed to be emitted before this `Appended` event.
    Appended(LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// Emitted whenever a non-ignored entry in `area` is forgotten via `Store::forget_entry`. No corresponding `PayloadForgotten` event is emitted in this case.
    EntryForgotten {
        /// The entry that was forgotten.
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
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

/// Describes which entries to ignore during a query.
///
/// The `Default::default()` ignores nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub struct QueryIgnoreParams {
    /// Omit entries with locally incomplete corresponding payloads.
    pub ignore_incomplete_payloads: bool,
    /// Omit entries whose payload is the empty string.
    pub ignore_empty_payloads: bool,
}

impl Default for QueryIgnoreParams {
    fn default() -> Self {
        Self {
            ignore_incomplete_payloads: false,
            ignore_empty_payloads: false,
        }
    }
}

/// The origin of an entry ingestion event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum EntryOrigin {
    /// The entry was probably created on this machine.
    Local,
    /// The entry was sourced from another source with an ID assigned by us.
    /// This is useful if you want to suppress the forwarding of entries to the peers from which the entry was originally sourced.
    Remote(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventSenderError<OE> {
    /// The store threw an error.
    StoreError(OE),
    /// You failed to process events quickly enough.
    DoTryToKeepUp,
}

impl<OE: Display + Error> Display for EventSenderError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSenderError::DoTryToKeepUp => {
                write!(f, "Had to terminate an event subscription because the subscriber could not keep up.")
            }
            EventSenderError::StoreError(err) => Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for EventSenderError<OE> {}

/// A [`Store`] is a set of [`AuthorisedEntry`] belonging to a single namespace, and a  (possibly partial) corresponding set of payloads.
pub trait Store<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
    type Error: Display + Error + PartialEq;

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

    /// Attempts to append part of a payload for a given [`AuthorisedEntry`].
    ///
    /// Will fail if:
    ///
    /// - The payload digest of the entry at the given subspace_id and path does not have the supplied `expected_digest` (if one was supplied).
    /// - The payload digest is not referred to by any of the store's entries.
    /// - A complete payload with the same digest is already held in storage.
    /// - The payload source produced more bytes than were expected for this payload.
    /// - The final payload's digest did not match the expected digest
    /// - Something else went wrong, e.g. there was no space for the payload on disk.
    ///
    /// This method **cannot** verify the integrity of partial payloads. This means that arbitrary (and possibly malicious) payloads smaller than the expected size will be stored unless partial verification is implemented upstream (e.g. during [the Willow General Sync Protocol's payload transformation](https://willowprotocol.org/specs/sync/index.html#sync_payloads_transform)).
    fn append_payload<Producer, PayloadSourceError>(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
        payload_source: &mut Producer,
    ) -> impl Future<
        Output = Result<PayloadAppendSuccess, PayloadAppendError<PayloadSourceError, Self::Error>>,
    >
    where
        Producer: BulkProducer<Item = u8, Error = PayloadSourceError>;

    /// Locally forgets an entry with a given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) id, returning the forgotten entry, or an error if no entry with that path and subspace ID are held by this store. If an `expected_digest` is supplied and the entry turns out to not have that digest, then this method does nothing and reports an `ForgetEntryError::WrongEntry` error.
    ///
    /// If the entry in question is the last remaining reference in the store to a particular [`PayloadDigest`], that payload will be forgotten from the store (if present).
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> impl Future<Output = Result<(), ForgetEntryError<Self::Error>>>;

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

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, or an error if no entry with that path and subspace ID is held by this store or if the entry's payload corresponds to other entries. If an `expected_digest` is supplied and the entry turns out to not have that digest, then this method does nothing and reports a `ForgetPayloadError::WrongEntry` error.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> impl Future<Output = Result<(), ForgetPayloadError<Self::Error>>>;

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

    /// Returns a [`ufotofu::Producer`] of bytes for the payload corresponding to the given subspace id and path. If an `expected_digest` is supplied and the entry turns out to not have that digest, then this method does nothing and reports a `PayloadError::WrongEntry` error.
    fn payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> impl Future<
        Output = Result<
            Option<impl BulkProducer<Item = u8, Final = (), Error = Self::Error>>,
            PayloadError<Self::Error>,
        >,
    >;

    /// Returns a [`LengthyAuthorisedEntry`] with the given [`Path`] and [subspace](https://willowprotocol.org/specs/data-model/index.html#subspace) ID, if present.
    fn entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = Result<Option<LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>, Self::Error>,
    >;

    /// Queries which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Area`], returning a producer of [`LengthyAuthorisedEntry`] **produced in an arbitrary order decided by the store implementation**.
    fn query_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, Final = ()>,
            Self::Error,
        >,
    >;

    /// Subscribes to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`AreaOfInterest`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = impl Producer<
            Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
            Error = EventSenderError<Self::Error>,
        >,
    >;
}
