use ufotofu::nb::BulkProducer;

use crate::{
    entry::AuthorisedEntry,
    parameters::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId},
    LengthyEntry,
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

pub enum QueryOrder {
    Subspace,
    Path,
    Timestamp,
}

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
    ) -> Result<
        EntryIngestionSuccess<MCL, MCC, MPL, N, S, PD, AT>,
        EntryIngestionError<MCL, MCC, MPL, N, S, PD, AT>,
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
        expected_digest: PD,
        expected_size: u64,
        producer: &mut Producer,
    ) -> Result<PayloadAppendSuccess<MCL, MCC, MPL, N, S, PD>, PayloadAppendError>
    where
        Producer: BulkProducer<Item = u8>;
}
