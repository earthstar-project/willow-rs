use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, NamespaceId, PayloadDigest, ResumptionFailedError, Store, SubspaceId,
};

/// A [`Store`] capable of performing [3d range-based set reconciliation](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3_range_based_set_reconciliation).
pub trait RbsrStore<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT, FP>
where
    Self: Store<MCL, MCC, MPL, N, S, PD, AT>,
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Query which entries are [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by a [`Range3d`], returning a producer of [`LengthyAuthorisedEntry`].
    ///
    /// If `will_sort` is `true`, entries will be sorted ascendingly by subspace_id first, path second.
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn query_range(
        &self,
        range: Range3d<MCL, MCC, MPL, S>,
        will_sort: bool,
        ignore_incomplete_payloads: bool,
        ignore_empty_payloads: bool,
    ) -> Self::EntryProducer;

    /// Subscribe to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Range3d`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn subscribe_range(
        &self,
        range: Range3d<MCL, MCC, MPL, S>,
        ignore_incomplete_payloads: bool,
        ignore_empty_payloads: bool,
    ) -> Self::EventProducer;

    /// Attempt to resume a subscription using a *progress ID* obtained from a previous subscription, or return an error if this store implementation is unable to resume the subscription.
    fn resume_range_subscription(
        &self,
        progress_id: u64,
        range: Range3d<MCL, MCC, MPL, S>,
        ignore_incomplete_payloads: bool,
        ignore_empty_payloads: bool,
    ) -> Result<Self::EventProducer, ResumptionFailedError>;

    /// Summarise a [`Range3d`] as a [fingerprint](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3rbsr_fp).
    fn summarise(&self, range: Range3d<MCL, MCC, MPL, S>) -> FP;

    /// Convert an [`AreaOfInterest`] to a concrete [`Range3d`] including all the entries the given [`AreaOfInterest`] would.
    fn area_of_interest_to_range(
        &self,
        aoi: AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> Range3d<MCL, MCC, MPL, S>;

    /// Partition a [`Range3d`] into `K` parts, or return `None` if the given range cannot be split into `K` many parts (for instance because the range only includes a single entry).
    fn partition_range<const K: usize>(
        &self,
        range: Range3d<MCL, MCC, MPL, S>,
    ) -> Option<[Range3d<MCL, MCC, MPL, S>; K]>;
}
