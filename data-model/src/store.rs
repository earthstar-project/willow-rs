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
    Success,
    /// The entry was successfully ingested and prefix pruned some entries.
    SuccessAndPruned(AEI),
    /// The entry was not ingested because a newer entry with same prfieiuhnsuthaeusntaheouonsth
    Obsolete {
        /// The obsolete entry which was not ingested.
        obsolete: AuthorisedEntry<N, S, P, PD, AT>,
        /// The newer entry which was not overwritten.
        newer: AuthorisedEntry<N, S, P, PD, AT>,
    },
}

pub enum EntryIngestionError<N: NamespaceId, S: SubspaceId, P: Path, PD: PayloadDigest, AT> {
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
    // associtade type named AEI, which must implement the ufotofu producer trait / or std Iterator
    // associated type of something went wrong error.

    /// Attempt to store an [`AuthorisedEntry`] in the [`Store`].
    fn ingest_entry(
        // this will end up not working but in theory it's right. ask aljoscha later.
        // later came.
        &self,
        authorised_entry: AuthorisedEntry<N, S, P, PD, AT>,
    ) -> Result<EntryIngestionSuccess<N, S, P, PD, AT, AEI>, EntryIngestionError<N, S, P, PD, AT>>;

    // commit / barf - ingest becomes queue_ingestion_entry?
    // we want bulk ingestion of entries too
    // forget_entry
    // forget_range
    // forget_area

    /// Attempt to ingest a payload for a given [`AuthorisedEntry`].
    fn ingest_payload(
        &self,
        expected_digest: PD,
        payload: Payload,
    ) -> Result<(), PayloadIngestionError>;

    /// Query the store's set of [`AuthorisedEntry`] using an [`Area`].
    fn query_area(&self, area: Area<S, P>, order: QueryOrder, reverse: bool) -> AEI;

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
