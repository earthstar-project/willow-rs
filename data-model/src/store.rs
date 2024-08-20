use crate::{
    entry::{AuthorisedEntry, Entry},
    grouping::{area::Area, area_of_interest::AreaOfInterest, range_3d::Range3d},
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

// eheheheh
type Payload = String;

/// Returned when an entry is successfully ingested into a [`Store`].
pub enum EntryIngestionSuccess<
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest,
    AT,
    AEI: Iterator<Item = AuthorisedEntry<N, S, P, PD, AT>>,
> {
    /// The entry was successfully ingested.
    Success(AuthorisedEntry<N, S, P, PD, AT>),
    /// The entry was successfully ingested and prefix pruned some entries.
    SuccessAndPruned(AEI),
}

pub enum EntryIngestionError<N: NamespaceId, S: SubspaceId, P: Path, PD: PayloadDigest, AT> {
    NewerPrefix {
        /// The obsolete entry which is pruned by the newer entry.
        pruned: AuthorisedEntry<N, S, P, PD, AT>,
        /// The newer entry whose path prefixes the obsolete entry's.
        newer: AuthorisedEntry<N, S, P, PD, AT>,
    },
    Obsolete {
        /// The obsolete entry which was not ingested.
        obsolete: AuthorisedEntry<N, S, P, PD, AT>,
        /// The newer entry which was not overwritten.
        newer: AuthorisedEntry<N, S, P, PD, AT>,
    },
    /// The entry belonged to another namespace.
    WrongNamespace(AuthorisedEntry<N, S, P, PD, AT>),
}

/// Returned when a [`Payload`] fails to be ingested into the [`Store`the ]
pub enum PayloadIngestionError {
    /// None of the entries in the store reference this payload.
    NotEntryReference,
    /// The payload is already held in storage.
    AlreadyHaveIt,
    /// The received's payload digest is not what was expected.
    DigestMismatch,
    /// Try deleting some files!!!
    SomethingElseWentWrong,
}

pub enum QueryOrder {
    Subspace,
    Path,
    Timestamp,
}

/// A [`Store`] is a set of [`AuthorisedEntry`].
pub trait Store<
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest,
    AT,
    AEI: Iterator<Item = AuthorisedEntry<N, S, P, PD, AT>>,
    // Do we need a trait for fingerprints?
    FP,
>
{
    /// Attempt to store an [`AuthorisedEntry`] in the [`Store`].
    fn ingest_entry(
        &self,
        expected_digest: PD,
        authorised_entry: AuthorisedEntry<N, S, P, PD, AT>,
    ) -> Result<EntryIngestionSuccess<N, S, P, PD, AT, AEI>, EntryIngestionError<N, S, P, PD, AT>>;
    // I'm told this is too complex, but I'm also told trait bounds can't be enforced on type aliases.

    /// Attempt to ingest a payload for a given [`AuthorisedEntry`].
    fn ingest_payload(&self, payload: Payload) -> Result<(), PayloadIngestionError>;

    /// Query the store's set of [`AuthorisedEntry`] using an [`Area`].
    fn query_area(&self, area: Area<S, P>) -> AEI;

    /// Query the store's set of [`AuthorisedEntry`] using a [`Range3d`].
    fn query_range(&self, area: Range3d<S, P>, order: QueryOrder, reverse: bool) -> AEI;

    /// Try to get the payload for a given [`Entry`].
    fn getPayload(&self, entry: &Entry<N, S, P, PD>) -> Option<Payload>;

    /// Summarise a given range into a fingerprint.
    fn summarise(&self, range: Range3d<S, P>) -> FP;

    /// Split a range into two approximately equally sized ranges, or return the original range if it can't be split.
    fn split_range(
        &self,
        range: Range3d<S, P>,
    ) -> Result<(Range3d<S, P>, Range3d<S, P>), Range3d<S, P>>;

    /// Transform an [`AreaOfInterest`] into a concrete range by using its `max_count` and `max_size` properties as clamps.
    fn aoi_to_range(&self, aoi: AreaOfInterest<S, P>) -> Range3d<S, P>;
}
