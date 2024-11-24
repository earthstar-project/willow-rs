use std::future::Future;

use async_cell::unsync::AsyncCell;
use futures::try_join;

use receive_prelude::{ReceivePreludeError, ReceivedPrelude};
use ufotofu::local_nb::{BulkConsumer, BulkProducer, Producer};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    ResumptionFailedError, Store, StoreEvent, SubspaceId,
};

mod parameters;
pub use parameters::*;

mod messages;
pub use messages::*;

mod commitment_scheme;
pub use commitment_scheme::*;
use commitment_scheme::{receive_prelude::receive_prelude, send_prelude::send_prelude};

/// An error which can occur during a WGPS synchronisation session.
pub enum WgpsError<E> {
    /// The received max payload power was invalid, i.e. greater than 64.
    PreludeMaxPayloadInvalid,
    /// The transport stopped producing bytes before it could be deemed ready.
    PreludeFinishedTooSoon,
    /// The underlying transport emitted an error.
    Transport(E),
}

impl<E> From<E> for WgpsError<E> {
    fn from(value: E) -> Self {
        Self::Transport(value)
    }
}

impl<E> From<ReceivePreludeError<E>> for WgpsError<E> {
    fn from(value: ReceivePreludeError<E>) -> Self {
        match value {
            ReceivePreludeError::Transport(err) => err.into(),
            ReceivePreludeError::MaxPayloadInvalid => WgpsError::PreludeMaxPayloadInvalid,
            ReceivePreludeError::FinishedTooSoon => WgpsError::PreludeFinishedTooSoon,
        }
    }
}

impl<E: core::fmt::Display> core::fmt::Display for WgpsError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WgpsError::Transport(transport_error) => write!(f, "{}", transport_error),
            WgpsError::PreludeMaxPayloadInvalid => write!(
                f,
                "The peer sent an invalid max payload power in their prelude."
            ),
            WgpsError::PreludeFinishedTooSoon => write!(
                f,
                "The peer terminated the connection before sending their full prelude."
            ),
        }
    }
}

pub struct SyncOptions<const CHALLENGE_LENGTH: usize> {
    max_payload_power: u8,
    challenge_nonce: [u8; CHALLENGE_LENGTH],
}

pub async fn sync_with_peer<
    const CHALLENGE_LENGTH: usize,
    const CHALLENGE_HASH_LENGTH: usize,
    CH: ChallengeHash<CHALLENGE_LENGTH, CHALLENGE_HASH_LENGTH>,
    E,
    C: BulkConsumer<Item = u8, Error = E>,
    P: BulkProducer<Item = u8, Error = E>,
>(
    options: &SyncOptions<CHALLENGE_LENGTH>,
    consumer: C,
    mut producer: P,
) -> Result<(), WgpsError<E>> {
    // Compute the commitment to our [nonce](https://willowprotocol.org/specs/sync/index.html#nonce); we send this
    // commitment to the peer at the start (the *prelude*) of the sync session.
    let our_commitment = CH::hash(&options.challenge_nonce);
    
    let mut consumer = consumer; // TODO turn into a SharedConsumer in an Arc here. See util::shared_encoder on the logical_channels branch (and ask Aljoscha about it).

    // This is set to `()` once our own prelude has been sent.
    let sent_own_prelude = AsyncCell::<()>::new();
    // This is set to the prelude received from the peer once it has arrived.
    let received_prelude = AsyncCell::<ReceivedPrelude<CHALLENGE_HASH_LENGTH>>::new();

    // Every unit of work that the WGPS needs to perform is defined as a future in the following, via an async block.
    // If one of these futures needs another unit of work to have been completed, this should be enforced by
    // calling `some_cell.get().await` for one of the cells defined above. Generally, these futures should *not* call
    // `some_cell.take().await`, since that might mess with other futures depending on the same step to have completed.
    //
    // Each of the futures must evaluate to a `Result<(), WgpsError<E>>`.
    // Since type annotations for async blocks are not possible in today's rust, we instead provide
    // a type annotation on the return value; that's why the last two lines of each of the following
    // async blocks are a weirdly overcomplicated way of returning `Ok(())`.

    let do_send_prelude = async {
        send_prelude(options.max_payload_power, &our_commitment, &mut consumer).await?;
        sent_own_prelude.set(());

        let ret: Result<(), WgpsError<E>> = Ok(());
        ret
    };

    let do_receive_prelude = async {
        received_prelude.set(receive_prelude(&mut producer).await?);

        let ret: Result<(), WgpsError<E>> = Ok(());
        ret
    };

    // Add each of the futures here. The macro polls them all to completion, until the first one hits
    // an error, in which case this function immediately returns with that first error.
    let ((), ()) = try_join!(do_send_prelude, do_receive_prelude,)?;
    Ok(())
}

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
