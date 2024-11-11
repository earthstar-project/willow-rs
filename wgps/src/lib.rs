use std::future::Future;

use ufotofu::local_nb::Producer;
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    ResumptionFailedError, Store, StoreEvent, SubspaceId,
};

mod commitment_scheme;
pub use commitment_scheme::*;

/// Options to specify how ranges should be partitioned.
#[derive(Debug, Clone, Copy)]
pub struct PartitionOpts {
    /// The largest number of entries that can be included by a range before it is better to send that range's fingerprint instead of sending its entries.
    pub max_range_size: usize,
    /// The maximum number of partitions to split a range into. Must be at least 2.
    pub max_splits: usize,
}

/// A split range and the action which should be taken with that split range during range-based set reconciliation.
pub type RangeSplit<const MCL: usize, const MCC: usize, const MPL: usize, S, FP> =
    (Range3d<MCL, MCC, MPL, S>, SplitAction<FP>);

/// Whether to send a split range's fingerprint or its included entries.
#[derive(Debug)]
pub enum SplitAction<FP> {
    SendFingerprint(FP),
    SendEntries(u64),
}

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
        range: &Range3d<MCL, MCC, MPL, S>,
        will_sort: bool,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Subscribe to events concerning entries [included](https://willowprotocol.org/specs/grouping-entries/index.html#area_include) by an [`Range3d`], returning a producer of `StoreEvent`s which occurred since the moment of calling this function.
    ///
    /// If `ignore_incomplete_payloads` is `true`, the producer will not produce entries with incomplete corresponding payloads.
    /// If `ignore_empty_payloads` is `true`, the producer will not produce entries with a `payload_length` of `0`.
    fn subscribe_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>;

    /// Attempt to resume a subscription using a *progress ID* obtained from a previous subscription, or return an error if this store implementation is unable to resume the subscription.
    fn resume_range_subscription(
        &self,
        progress_id: u64,
        range: &Range3d<MCL, MCC, MPL, S>,
        ignore: Option<QueryIgnoreParams>,
    ) -> impl Future<
        Output = Result<
            impl Producer<Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>>,
            ResumptionFailedError,
        >,
    >;

    /// Summarise a [`Range3d`] as a [fingerprint](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#d3rbsr_fp).
    fn summarise(&self, range: Range3d<MCL, MCC, MPL, S>) -> impl Future<Output = FP>;

    /// Convert an [`AreaOfInterest`] to a concrete [`Range3d`] including all the entries the given [`AreaOfInterest`] would.
    fn area_of_interest_to_range(
        &self,
        aoi: &AreaOfInterest<MCL, MCC, MPL, S>,
    ) -> impl Future<Output = Range3d<MCL, MCC, MPL, S>>;

    /// Partition a [`Range3d`] into many parts, or return `None` if the given range cannot be split (for instance because the range only includes a single entry).
    fn partition_range(
        &self,
        range: &Range3d<MCL, MCC, MPL, S>,
        options: &PartitionOpts,
    ) -> impl Future<Output = Option<impl Iterator<Item = RangeSplit<MCL, MCC, MPL, S, FP>>>>;
}
