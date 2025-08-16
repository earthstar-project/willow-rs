use std::{
    cell::RefCell,
    collections::VecDeque,
    error::Error,
    fmt::{Debug, Display},
    future::Future,
    rc::Rc,
};

use either::Either::{self, Left, Right};
use slab::Slab;
use ufotofu::{BulkProducer, Producer};
use wb_async_utils::TakeCell;

use crate::{
    entry::AuthorisedEntry, grouping::Area, AuthorisationToken, Entry, LengthyAuthorisedEntry,
    NamespaceId, Path, PayloadDigest, SubspaceId,
};

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

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
    /// Something specific to this store implementation went wrong.
    OperationsError(OE),
}

impl<OE: Display + Error> Display for EntryIngestionError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryIngestionError::PruningPrevented => {
                write!(f, "Entry ingestion would have triggered undesired pruning.")
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
    /// No entry for the given subspace and path exists in this store.
    NoSuchEntry,
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
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
            PayloadAppendError::NoSuchEntry => {
                write!(
                    f,
                    "No entry for the given subspace and path exists in this store"
                )
            }
            PayloadAppendError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to append to had an unexpected payload_digest."
                )
            }
            PayloadAppendError::TooManyBytes => write!(
                f,
                "The payload source produced more bytes than were expected for this payload."
            ),
            PayloadAppendError::DigestMismatch => {
                write!(f, "The complete payload's digest is not what was expected.")
            }
            PayloadAppendError::SourceError { source_error, .. } => {
                write!(f, "The payload source emitted an error: {source_error}")
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
                write!(f, "The entry to forget had an unexpected payload_digest.")
            }
            ForgetEntryError::OperationError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<OE: Display + Error> Error for ForgetEntryError<OE> {}

/// Returned when forgetting a payload fails.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum ForgetPayloadError<OE> {
    /// No entry for the given subspace and path exists in this store.
    NoSuchEntry,
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for ForgetPayloadError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForgetPayloadError::NoSuchEntry => {
                write!(
                    f,
                    "No entry for the given subspace and path exists in this store"
                )
            }
            ForgetPayloadError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to forget had an unexpected payload_digest."
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
    /// No entry for the given subspace and path exists in this store.
    NoSuchEntry,
    /// The offset at which to fetch the payload bytes was too large.
    OutOfBounds,
    /// The operation supplied an expected payload_digest, but it did not match the digest of the entry.
    WrongEntry,
    /// Something specific to this store implementation went wrong.
    OperationError(OE),
}

impl<OE: Display + Error> Display for PayloadError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::NoSuchEntry => {
                write!(
                    f,
                    "Attempted to fetch a payload for which we do not even have an entry."
                )
            }
            PayloadError::OutOfBounds => {
                write!(
                    f,
                    "Attempted to fetch payload at an offset that was too large."
                )
            }
            PayloadError::WrongEntry => {
                write!(
                    f,
                    "The entry to whose payload to fetch to had an unexpected payload_digest."
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
    /// Emitted whenever an entry is inserted into the store that *might* cause pruning inside `area`. It is possible that no entry was actually pruned form the area, if nothing got overwritten.
    ///
    /// When the inserted entry falls into `area`, then the corresponding `PruneAlert` is always delivered *before* the corresponding `Ingested` event.
    PruneAlert {
        /// The entry that caused the pruning.
        cause: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
    /// An existing entry inside `area` received a portion of its corresponding payload.
    ///
    /// If `ignores.ignore_incomplete_payloads`, this is only emitted when the payload is now fully available. In this case, the corresponding `Ingested` event is guaranteed to be emitted before this `Appended` event.
    Appended {
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        /// How many bytes of the payload were available before this event.
        previous_available: u64,
        /// How many bytes of the payload are now available.
        now_available: u64,
    },
    /// Emitted whenever a non-ignored entry in `area` is forgotten via `Store::forget_entry`. No corresponding `PayloadForgotten` event is emitted in this case.
    EntryForgotten {
        /// The entry that was forgotten.
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
    /// Emitted whenever a call to `Store::forget_area` might affect `area`. No corresponding `AreaPayloadForgotten` event is emitted in this case.
    AreaForgotten {
        /// The area that was forgotten.
        area: Area<MCL, MCC, MPL, S>,
        /// A subarea that was retained (if any).
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    /// Emitted whenever the payload of a non-ignored entry in `area` is forgotten via `Store::forget_payload`. Emitted even if no payload bytes had been available to forget in the first place.
    PayloadForgotten(AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>),
    /// Emitted whenever a call to `Store::forget_area_payloads` might affect `area`.
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Default)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct QueryIgnoreParams {
    /// Omit entries with locally incomplete corresponding payloads.
    pub ignore_incomplete_payloads: bool,
    /// Omit entries whose payload is the empty string.
    pub ignore_empty_payloads: bool,
}

/// A payload associated with an [`Entry`].
pub enum Payload<E, P: BulkProducer<Item = u8, Final = (), Error = E>> {
    /// A payload for which all bytes are available locally.
    Complete(P),
    /// A payload for which fewer bytes than the known length of the payload are available locally (at the time at which the store was queried). If the payload becomes fully available before the user code has exhausted the producer, then this variant might indeed yield the complete payload despite its name.
    Incomplete(P),
}

impl QueryIgnoreParams {
    // An entry for which no payload bytes are available.
    fn ignores_fresh_entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>(
        &self,
        entry: &Entry<MCL, MCC, MPL, N, S, PD>,
    ) -> bool
    where
        S: PartialEq,
    {
        // Ignore if necessary if empty payload
        if self.ignore_empty_payloads && entry.payload_length() == 0 {
            return true;
        }

        // Ignore if necessary if the entry has an incomplete payload (always unless the expected payload length is zero).
        if self.ignore_incomplete_payloads && entry.payload_length() != 0 {
            return true;
        }

        false
    }

    fn ignores_lengthy_authorised_entry<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        PD,
        AT,
    >(
        &self,
        entry: &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        available: u64,
    ) -> bool
    where
        S: PartialEq,
    {
        // Ignore if necessary if empty payload
        if self.ignore_empty_payloads && entry.entry().payload_length() == 0 {
            return true;
        }

        // Ignore if necessary if the entry has an incomplete payload (always unless the expected payload length is zero).
        if self.ignore_incomplete_payloads && entry.entry().payload_length() != available {
            return true;
        }

        false
    }
}

/// The origin of an entry ingestion event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub enum EntryOrigin {
    /// The entry was probably created on this machine.
    Local,
    /// The entry was sourced from another source with an ID assigned by us.
    /// This is useful if you want to suppress the forwarding of entries to the peers from which the entry was originally sourced.
    Remote(u64),
}

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

    /// Attempts to append part of a payload for an entry at a given SubspaceId-Path-pair.
    ///
    /// Will report an error if:
    ///
    /// - There is no entry for the given SubspaceId-Path pair.
    /// - The payload digest of the entry at the given subspace_id and path is not equal to the supplied `expected_digest` (*if* one was supplied).
    /// - The payload source produced more bytes than were expected for this payload.
    /// - The payload source yielded an error.
    /// - The final payload's digest did not match the expected digest
    /// - Something else went wrong, e.g. there was no space for the payload on disk.
    ///
    /// This method **does not** and **cannot** verify the integrity of partial payloads. This means that arbitrary (and possibly malicious) payloads smaller than the expected size will be stored unless partial verification is implemented upstream (e.g. as part of a sync protocol).
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
    /// If the entry in question is the last remaining reference in the store to a particular [`crate::PayloadDigest`], that payload will be forgotten from the store (if present).
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> impl Future<Output = Result<(), ForgetEntryError<Self::Error>>>;

    /// Locally forgets all [`AuthorisedEntry`] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`crate::grouping::Area`], returning the number of forgotten entries.
    ///
    /// If forgetting many entries causes no there to be no remaining references to certain payload digests, those payloads will be removed (if present).
    ///
    /// If `protected` is `Some`, then all entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Locally forgets the corresponding payload of the entry with a given path and subspace, panics if no entry with that path and subspace ID is held by this store. If an `expected_digest` is supplied and the entry turns out to not have that digest, then this method does nothing and reports a `ForgetPayloadError::WrongEntry` error.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payload back.
    fn forget_payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        expected_digest: Option<PD>,
    ) -> impl Future<Output = Result<(), ForgetPayloadError<Self::Error>>>;

    /// Locally forgets all payloads with corresponding ['AuthorisedEntry'] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`crate::grouping::Area`], returning a count of forgotten payloads. Payloads corresponding to entries *outside* of the given `area` param will be be prevented from being forgotten.
    ///
    /// If `protected` is `Some`, then all payloads corresponding to entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by that [`Area`] will be prevented from being forgotten, even though they are included by `area`.
    ///
    /// Forgetting is not the same as [pruning](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning)! Subsequent joins with other [`Store`]s may bring the forgotten payloads back.
    fn forget_area_payloads(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        protected: Option<&Area<MCL, MCC, MPL, S>>,
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Forces persistence of all previous mutations
    fn flush(&self) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns a [`ufotofu::Producer`] of bytes for the payload corresponding to the given subspace id and path, starting at the supplied offset. If an `expected_digest` is supplied and the entry turns out to not have that digest, then this method does nothing and reports a `PayloadError::WrongEntry` error. If the supplied `offset` is equal to or greater than the number of available payload bytes, this reports a `PayloadError::OutOfBounds`.
    fn payload(
        &self,
        subspace_id: &S,
        path: &Path<MCL, MCC, MPL>,
        offset: u64,
        expected_digest: Option<PD>,
    ) -> impl Future<
        Output = Result<
            Payload<Self::Error, impl BulkProducer<Item = u8, Final = (), Error = Self::Error>>,
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

    /// Subscribes to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`crate::grouping::Area`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    fn subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = impl Producer<
            Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
            Final = (),
            Error = Self::Error,
        >,
    >;

    fn query_and_subscribe_area(
        &self,
        area: &Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> impl Future<
        Output = Result<
            impl Producer<
                Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
                Final = impl Producer<
                    Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
                    Final = (),
                    Error = Self::Error,
                >,
                Error = Self::Error,
            >,
            Self::Error,
        >,
    >;
}

//---------------------------//
// In-Memory Event Queue     //
//---------------------------//

// What follows is one possible technique for implementing the event subscription service offered by stores. This technique maintains a queue of (relevant) store operations. Subscribers maintain an offset into this queue; producing events works by advancing through the queue, ignoring irrelevant operations, and emitting events whenever appropriate. The queue has a maximum capacity, if it is reached, but some subscriber has not yet processed the oldest operation, then either the queue blocks or the subscriber is removed.

// A more sophisticated implementation could go beyond a mere queue and remove operations that have been obsoleted by later operations. We don't do this here. A later implementation that stores the queue on persistent storage *must* implement such optimisations however, since "deleted" data must also disappear from the operations queue.

/// An operation, as stored in the operations queue. Note that there is no op corresponding to the `PruneAlert` event, since those are generated from insertion ops. Note also that we use LengthyAuthorisedEntries instead of merely AuthorisedEntries for EntryForgotten and PayloadForgotten. This is so that we can respect QueryIgnoreParams.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum QueuedOp<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> {
    Insertion {
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        origin: EntryOrigin,
    },
    Appended {
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        previous_available: u64,
        now_available: u64,
    },
    EntryForgotten {
        entry: LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
    AreaForgotten {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
    PayloadForgotten {
        entry: LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
    AreaPayloadsForgotten {
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    },
}

// The store maintains a queue of `QueuedOp`s. Subscribers must be able to track their offset into the queue. But since items may be popped off, absolute offsets into the queue would be cumbersome. Instead, ops are addressed by a successively incremented counter (i.e., they get numbered sequentially). The store only needs to store the total number of ops that ever got popped off the queue in order to convert these absolute, unique, sequential op ids into local offsets in its queue.

// The interesting part is how we organise our subscribers. Considerations:
//
// - we need to cheaply add and remove subscribers
// - we need to cheaply query for the (a) subscriber which has the (a) lowest op id (so that we know whether we can pop off old events or not)
// - we need to notify subscribers which reached the end of the queue when another op has been pushed
//
// We can elegantly satisfy these requirements by organising subscribers in a doubly-linked list which we keep sorted by the op id up to which the subscribers have processed the operations. But. Doubly-linked lists in rust (and in general, for that matter) are an absolute pain, because ownership and stuff. Read https://rust-unofficial.github.io/too-many-lists/ if you do not know what I am talking about.

// So instead of the theoretically really nice solution, we'll hack something together. We store the subscribers in a [slab](https://docs.rs/slab/latest/slab/). When needing to pop an op, we do not remove any subscribers, their next attempt to produce an event will simply yield the final item of the producer. Dropping the user-facing part of the subscription also removes the internal part. For notifying up-to-date subscribers of a new push, simply iterate through the full slab. For realistic numbers of subscribers, this is probably not only "efficient enough", but actually significantly more performant than a doubly-linked list.

#[derive(Debug)]
pub struct EventSystem<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, Err> {
    op_queue: VecDeque<QueuedOp<MCL, MCC, MPL, N, S, PD, AT>>,
    // A statically set limit on how many ops to buffer at most at the same time.
    max_queue_capacity: usize,
    /// Total number of ops that have been popped of the op_queue so far
    popped_count: u64,
    subscribers: Slab<InternalSubscriber<Err>>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, Err>
    EventSystem<MCL, MCC, MPL, N, S, PD, AT, Err>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Creates a new Eventsystem.
    pub fn new(max_queue_capacity: usize) -> Self {
        Self {
            op_queue: VecDeque::new(),
            max_queue_capacity,
            popped_count: 0,
            subscribers: Slab::new(),
        }
    }

    /// Create a new subscription: setting up the internals, and returning the external part.
    pub fn add_subscription(
        this: Rc<RefCell<Self>>,
        area: Area<MCL, MCC, MPL, S>,
        ignore: QueryIgnoreParams,
    ) -> Subscriber<MCL, MCC, MPL, N, S, PD, AT, Err> {
        let cell = Rc::new(TakeCell::new());

        let internal = InternalSubscriber {
            next_op_id: cell.clone(),
        };
        let key = this.borrow_mut().subscribers.insert(internal);

        Subscriber {
            events: this,
            next_op_id: cell,
            slab_key: key,
            area,
            ignore,
            buffered_event: None,
        }
    }

    /// Call this inside your store impl after it has ingested an entry.
    pub fn ingested_entry(
        &mut self,
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        origin: EntryOrigin,
    ) {
        self.enqueue_op(QueuedOp::Insertion { entry, origin })
    }

    /// Call this inside your store impl after it has appended to a payload.
    pub fn appended_payload(
        &mut self,
        entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        previous_available: u64,
        now_available: u64,
    ) {
        self.enqueue_op(QueuedOp::Appended {
            entry,
            previous_available,
            now_available,
        })
    }

    /// Call this inside your store impl after it has forgotten an entry.
    pub fn forgot_entry(&mut self, entry: LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>) {
        self.enqueue_op(QueuedOp::EntryForgotten { entry })
    }

    /// Call this inside your store impl after it has forgotten an area.
    pub fn forgot_area(
        &mut self,
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) {
        self.enqueue_op(QueuedOp::AreaForgotten { area, protected })
    }

    /// Call this inside your store impl after it has forgotten a payload.
    pub fn forgot_payload(&mut self, entry: LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>) {
        self.enqueue_op(QueuedOp::PayloadForgotten { entry })
    }

    /// Call this inside your store impl after it has forgotten the payloads of an area.
    pub fn forgot_area_payloads(
        &mut self,
        area: Area<MCL, MCC, MPL, S>,
        protected: Option<Area<MCL, MCC, MPL, S>>,
    ) {
        self.enqueue_op(QueuedOp::AreaPayloadsForgotten { area, protected })
    }

    // We enqueue an operation. If the max capacity of the queue is reached through that, we pop the oldest op (which might cause straggling subscribers to be cancelled the next time they try to produce an event). If any subscribers have been awaiting a new op, we notify them.
    fn enqueue_op(&mut self, op: QueuedOp<MCL, MCC, MPL, N, S, PD, AT>) {
        // println!("enqueue_op: {:?}", op);
        self.op_queue.push_back(op);

        if self.op_queue.len() > self.max_queue_capacity {
            self.op_queue.pop_front();
        }

        for (_, sub) in self.subscribers.iter() {
            if sub.next_op_id.is_empty() {
                sub.next_op_id
                    .set(Ok(self.popped_count + (self.op_queue.len() as u64) - 1));
            }
        }
    }

    /// Given an op id, return the matching stored QueuedOp, or None if the id is too old (the corresponding op has already been popped).
    fn resolve_op_id(&self, id: u64) -> Option<&QueuedOp<MCL, MCC, MPL, N, S, PD, AT>> {
        match id.checked_sub(self.popped_count) {
            None => None,
            Some(index) => self.op_queue.get(index as usize),
        }
    }

    /// A function for debugging and testing purposes: signals to all subscribers that they lagged behind too far, irrespective of their actual ability to keep up. Might misbehave if the store performs operations after this but before the subscriber received the cancellation. Basically, don't use this, it is a hack to make certain tests work.
    pub fn cancel_all_subscriptions(&self) {
        for (_, sub) in self.subscribers.iter() {
            if sub.next_op_id.is_empty() {
                sub.next_op_id.set(Ok(u64::MAX));
            }
        }
    }
}

/// The internal part of a subscriber.
#[derive(Debug)]
pub struct InternalSubscriber<Err> {
    // This allows the public endpoint to await new entries. Stores Ok(op_id) of the next op_id to retrieve, stores Err(err) to emit an error err, is empty while the subscriber is fully up to date.
    next_op_id: Rc<TakeCell<Result<u64, Err>>>,
}

/// The public-facing part of a subscriber (to be returned by Store::subscribe_area).
#[derive(Debug)]
pub struct Subscriber<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, Err> {
    events: Rc<RefCell<EventSystem<MCL, MCC, MPL, N, S, PD, AT, Err>>>,
    // Shared with InternalSubscriber.next_op_id.
    next_op_id: Rc<TakeCell<Result<u64, Err>>>,
    /// The key by which the internal subscriber part is stored in the EventSystem. Upon dropping, the Subscriber, the corresponding InternalSubscriber is removed from the slab.
    slab_key: usize,
    area: Area<MCL, MCC, MPL, S>,
    ignore: QueryIgnoreParams,
    /// Some store ops trigger *two* events. In those cases, the second event is stored here. Each call to `produce` checks for a buffered event first before continuing to process the op queue.
    buffered_event: Option<StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, Err> Drop
    for Subscriber<MCL, MCC, MPL, N, S, PD, AT, Err>
{
    fn drop(&mut self) {
        self.events.borrow_mut().subscribers.remove(self.slab_key);
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, Err> Producer
    for Subscriber<MCL, MCC, MPL, N, S, PD, AT, Err>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    type Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>;

    type Final = ();

    type Error = Err;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if let Some(buffered) = self.buffered_event.take() {
            return Ok(Left(buffered));
        }

        // We loop and skip over events that we ignore.
        // We exit the loop when no more events are available or upon hitting an event we do not ignore.
        loop {
            match self.next_op_id.take().await {
                Err(err) => return Err(err),
                Ok(op_id) => {
                    match self.events.borrow().resolve_op_id(op_id) {
                        None => {
                            // We lag too far behind.
                            return Ok(Right(()));
                        }
                        Some(op) => {
                            // Advance the op_id.
                            if op_id + 1
                                == self.events.borrow().popped_count
                                    + (self.events.borrow().op_queue.len() as u64)
                            {
                                // reached the end of the op queue. Do nothing, the next event insertion will fill self.next_op_id
                            } else {
                                self.next_op_id.set(Ok(op_id + 1));
                            }

                            match op {
                                QueuedOp::Appended {
                                    entry,
                                    previous_available,
                                    now_available,
                                } => {
                                    if !self.area.includes_entry(entry.entry())
                                        || self
                                            .ignore
                                            .ignores_lengthy_authorised_entry(entry, *now_available)
                                    {
                                        continue;
                                    }

                                    if self.ignore.ignore_incomplete_payloads {
                                        // If the entry was ignored due to an incomplete payload, we buffer the append event and emit an insertion event first.
                                        self.buffered_event = Some(StoreEvent::Appended {
                                            entry: entry.clone(),
                                            previous_available: *previous_available,
                                            now_available: *now_available,
                                        });

                                        return Ok(Left(StoreEvent::Ingested {
                                            entry: entry.clone(),
                                            origin: EntryOrigin::Local,
                                        }));
                                    } else {
                                        // Otherwise, emit the Appended event directly.
                                        return Ok(Left(StoreEvent::Appended {
                                            entry: entry.clone(),
                                            previous_available: *previous_available,
                                            now_available: *now_available,
                                        }));
                                    }
                                }
                                QueuedOp::AreaForgotten { area, protected } => {
                                    if area.intersection(&self.area).is_some() {
                                        if let Some(prot) = protected {
                                            if prot.includes_area(&self.area) {
                                                // continue with area, since the subscribed area is fully protected
                                                continue;
                                            }
                                        }

                                        return Ok(Left(StoreEvent::AreaForgotten {
                                            area: area.clone(),
                                            protected: protected.clone(),
                                        }));
                                    } else {
                                        // no-op, continue with next event
                                    }
                                }
                                QueuedOp::AreaPayloadsForgotten { area, protected } => {
                                    if area.intersection(&self.area).is_some() {
                                        if let Some(prot) = protected {
                                            if prot.includes_area(&self.area) {
                                                // continue with area, since the subscribed area is fully protected
                                                continue;
                                            }
                                        }

                                        return Ok(Left(StoreEvent::AreaPayloadsForgotten {
                                            area: area.clone(),
                                            protected: protected.clone(),
                                        }));
                                    } else {
                                        // no-op, continue with next event
                                    }
                                }
                                QueuedOp::EntryForgotten { entry } => {
                                    if self.area.includes_entry(entry.entry().entry())
                                        && !self.ignore.ignores_lengthy_authorised_entry(
                                            entry.entry(),
                                            entry.available(),
                                        )
                                    {
                                        return Ok(Left(StoreEvent::EntryForgotten {
                                            entry: entry.entry().clone(),
                                        }));
                                    } else {
                                        // no-op, continue with next event
                                    }
                                }

                                QueuedOp::Insertion { entry, origin } => {
                                    // println!(
                                    //     "Subscriber looking at a QueuedOp::Insertion: {:?}",
                                    //     entry
                                    // );

                                    // Is the entry in the subscribed-to area?
                                    if self.area.includes_entry(entry.entry()) {
                                        // println!("Entry included in area {:?}", self.area);

                                        if self.ignore.ignores_fresh_entry(entry.entry()) {
                                            // println!("Entry got ignored, emitting a PruneAlert event for it.");

                                            return Ok(Left(StoreEvent::PruneAlert {
                                                cause: entry.clone(),
                                            }));
                                        } else {
                                            // println!("Entry not ignored, emitting a PruneAlert followed by an IngestedEvent.");

                                            // Insertion is not ignored.
                                            // Buffer the actual insertion event, then emit the prune alert.
                                            self.buffered_event = Some(StoreEvent::Ingested {
                                                entry: entry.clone(),
                                                origin: *origin,
                                            });

                                            return Ok(Left(StoreEvent::PruneAlert {
                                                cause: entry.clone(),
                                            }));
                                        }
                                    } else if self.area.could_be_pruned_by(entry.entry()) {
                                        // println!("Insertion outside area but might still prune something inside the area");

                                        // Insertion outside area but might still prune something inside the area.
                                        return Ok(Left(StoreEvent::PruneAlert {
                                            cause: entry.clone(),
                                        }));
                                    } else {
                                        // println!("Insertion does not affect the area, continue with next event");

                                        // no-op, insertion does not affect the area, continue with next event
                                    }
                                }

                                QueuedOp::PayloadForgotten { entry } => {
                                    if self.area.includes_entry(entry.entry().entry())
                                        && !self.ignore.ignores_lengthy_authorised_entry(
                                            entry.entry(),
                                            entry.available(),
                                        )
                                    {
                                        return Ok(Left(StoreEvent::PayloadForgotten(
                                            entry.entry().clone(),
                                        )));
                                    } else {
                                        // no-op, continue with next event
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
