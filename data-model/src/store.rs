use std::future::Future;

use ufotofu::nb::BulkProducer;

use crate::{
    entry::AuthorisedEntry,
    grouping::AreaOfInterest,
    parameters::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId},
    LengthyEntry, Path,
};

/// Returned when an entry is successfully ingested into a [`Store`].
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
    /// The entry was successfully ingested and prefix pruned some entries.
    SuccessAndPruned(Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>),
    /// The entry was not ingested because a newer entry with same
    Obsolete {
        /// The obsolete entry which was not ingested.
        obsolete: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        /// The newer entry which was not overwritten.
        newer: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    },
}

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
pub struct NoSuchEntryError();

/// A [`Store`] is a set of [`AuthorisedEntry`] belonging to a single namespace, and a  (possibly partial) corresponding set of payloads.
pub trait Store<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// The [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) which all of this store's [`AuthorisedEntry`] belong to.
    fn namespace_id() -> N;

    /// Attempt to store an [`AuthorisedEntry`] in the [`Store`].
    /// Will fail if the entry belonged to a different namespace than the store's.
    fn ingest_entry(
        &self,
        authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    ) -> impl Future<
        Output = Result<
            EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
            EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT>,
        >,
    >;

    // TODO: Bulk ingestion entry. The annoying there is it needs its own success / error types. We could get around this by exposing a BulkConsumer from the Store, but [then we need to support multi-writer consumption](https://github.com/earthstar-project/willow-rs/pull/21#issuecomment-2192393204).

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
        expected_digest: PD,
        expected_size: u64,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<PayloadAppendSuccess<MCL, MCC, MPL, N, S, PD>, PayloadAppendError>>
    where
        Producer: BulkProducer<Item = u8>;

    /// Locally forget an entry with a given [path] and [subspace] id, returning the forgotten entry, or an error if no entry with that path and subspace ID are held by this store.
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the entry. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entry back.
    fn forget_entry(
        path: &Path<MCL, MCC, MPL>,
        subspace_id: S,
        traceless: bool,
    ) -> impl Future<Output = Result<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, NoSuchEntryError>>;

    /// Locally forget all [`AuthorisedEntry`] [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all forgotten entries
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten entries. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_area(
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;

    /// Locally forget all [`AuthorisedEntry`] **not** [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a given [`AreaOfInterest`], returning all forgotten entries
    ///
    /// If the `traceless` parameter is `true`, the store will keep no record of ever having had the forgotten entries. If `false`, it *may* persist what was forgetten for an arbitrary amount of time.
    ///
    /// Forgetting is not the same as deleting! Subsequent joins with other [`Store`]s may bring the forgotten entries back.
    fn forget_everything_but_area(
        area: &AreaOfInterest<MCL, MCC, MPL, S>,
        traceless: bool,
    ) -> impl Future<Output = Vec<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>>;
}
