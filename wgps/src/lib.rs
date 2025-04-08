use std::{future::Future, ops::DerefMut};

//use lcmux::new_lcmux;
use ufotofu::{BulkConsumer, BulkProducer, Producer};
use wb_async_utils::{Mutex, OnceCell};
use willow_data_model::{
    grouping::{AreaOfInterest, Range3d},
    AuthorisationToken, LengthyAuthorisedEntry, NamespaceId, PayloadDigest, QueryIgnoreParams,
    Store, StoreEvent, SubspaceId,
};

mod parameters;
pub use parameters::*;

mod messages;
pub use messages::*;

mod commitment_scheme;
use commitment_scheme::*;
use ufotofu_codec::{DecodableCanonic, DecodeError, Encodable};

mod data_handles;

/// An error which can occur during a WGPS synchronisation session.
pub enum WgpsError<E> {
    /// The received payload was invalid.
    PreludeInvalid,
    /// The underlying transport emitted an error.
    Transport(E),
}

impl<E> From<E> for WgpsError<E> {
    fn from(value: E) -> Self {
        Self::Transport(value)
    }
}

impl<E: core::fmt::Display> core::fmt::Display for WgpsError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WgpsError::Transport(transport_error) => write!(f, "{}", transport_error),
            WgpsError::PreludeInvalid => write!(f, "The peer sent an invalid prelude."),
        }
    }
}

pub struct SyncOptions<const CHALLENGE_LENGTH: usize> {
    max_payload_power: u8,
    challenge_nonce: [u8; CHALLENGE_LENGTH],
    /// For all lcmux buffers.
    max_queue_capacity: usize,
    /// Only start sending guarantees once the buffer capacity of a logical channel reaches this value.
    /// Must be at most the max_queue_capacity, or we will never grant any guarantees.
    watermark: usize,
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
    consumer: Mutex<C>,
    producer: Mutex<P>,
) -> Result<(), WgpsError<E>> {
    // This is set to `()` once our own prelude has been sent.
    let sent_own_prelude = OnceCell::<()>::new();
    // This is set to the prelude received from the peer once it has arrived.
    let received_prelude = OnceCell::<Prelude<CHALLENGE_HASH_LENGTH>>::new();

    // Every unit of work that the WGPS needs to perform is defined as a future in the following, via an async block.
    // If one of these futures needs another unit of work to have been completed, this should be enforced by
    // calling `some_cell.get().await` for one of the cells defined above.
    //
    // Each of the futures must evaluate to a `Result<(), WgpsError<E>>`.
    // Since type annotations for async blocks are not possible in today's rust, we instead provide
    // a type annotation on the return value; that's why the last two lines of each of the following
    // async blocks are a weirdly overcomplicated way of returning `Ok(())`.

    let _send_prelude = async {
        let own_prelude = Prelude {
            max_payload_power: options.max_payload_power,
            commitment: CH::hash(&options.challenge_nonce), // Hash our challenge nonce to obtain the commitment to send.
        };

        // Encode our prelude and transmit it.
        let mut c = consumer.write().await;
        own_prelude.encode(c.deref_mut()).await?;

        // let mut encoder = SharedEncoder::new(&consumer);
        // encoder.consume(own_prelude).await?;

        // Notify other tasks that our prelude has been successfully sent.
        let _ = sent_own_prelude.set(());

        let ret: Result<(), WgpsError<E>> = Ok(());
        ret
    };

    let _receive_prelude = async {
        let mut p = producer.write().await;

        match Prelude::decode_canonic(p.deref_mut()).await {
            Ok(prelude) => {
                let _ = received_prelude.set(prelude);
            }
            Err(DecodeError::Producer(err)) => return Err(WgpsError::Transport(err)),
            Err(_) => return Err(WgpsError::PreludeInvalid),
        }

        let ret: Result<(), WgpsError<E>> = Ok(());
        ret
    };

    /*
    let lcmux_handling = async {
        // Everything in here needs to use lcmux.
        // TODO replace the `i16` with an enum of all supported control messages to receive, once we support any!

        let mut lcmux = new_lcmux::<7, _, _, i16>(
            &producer,
            &consumer,
            options.max_queue_capacity,
            options.watermark,
        );

        let reveal_commitment = async {
            // Before revealing our commitment, we must have both sent our own prelude and received our peer's prelude.
            let _ = sent_own_prelude.get().await;
            let _ = received_prelude.get().await;

            let msg = CommitmentReveal {
                nonce: &options.challenge_nonce,
            };
            lcmux.control_message_sender.receive(msg).await?;

            let ret: Result<(), WgpsError<E>> = Ok(());
            ret
        };

        let _ = try_join!(reveal_commitment)?; // TODO add futures for the remaining, as of yet unimplemented tasks: rbsr, pai, etc. Will be fleshed out as we progress with the WGPS implementation.
        Ok(())
    };

    // Add each of the futures here. The macro polls them all to completion, until the first one hits
    // an error, in which case this function immediately returns with that first error.
    let ((), (), ()) = try_join!(send_prelude, receive_prelude, lcmux_handling)?;
    */
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
