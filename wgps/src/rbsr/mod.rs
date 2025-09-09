use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

use either::Either::{Left, Right};
use lcmux::{ChannelSender, LogicalChannelClientError};
use ufotofu::{BulkConsumer, Producer};
use wb_async_utils::Mutex;
use willow_data_model::grouping::Range3d;
use willow_data_model::{
    AuthorisationToken, AuthorisedEntry, LengthyEntry, QueryIgnoreParams, StoreEvent,
};

use crate::messages::{
    RangeInfo, ReconciliationAnnounceEntries, ReconciliationSendEntry,
    ReconciliationSendFingerprint, ReconciliationTerminatePayload,
};
use crate::parameters::{WgpsFingerprint, WgpsNamespaceId, WgpsPayloadDigest, WgpsSubspaceId};
use crate::{pio, storedinator::Storedinator};
use crate::{RbsrStore, SplitAction};

/// The state per WGPS session for handling RBSR.
pub(crate) struct ReconciliationSender<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
    MyReadCap,
    MyEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
    Store,
    StoreCreationFunction,
    Fingerprint,
> {
    /// Need this to decode messages and to check validity of incoming messages (whether their capability-handle pairs are valid and refer to overlapping areas).
    pio_state: Rc<
        pio::State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            MyReadCap,
            MyEnumCap,
            P,
            PFinal,
            PErr,
            C,
            CErr,
        >,
    >,
    /// Access to our stores.
    storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
    reconciliation_channel_sender: Mutex<ChannelSender<5, P, PFinal, PErr, C, CErr>>,
    data_channel_sender: Rc<Mutex<ChannelSender<5, P, PFinal, PErr, C, CErr>>>,
    session_id: u64,
    previously_sent_fingerprint_range: Mutex<Range3d<MCL, MCC, MPL, S>>,
    previously_sent_itemset_range: Mutex<Range3d<MCL, MCC, MPL, S>>,
    previously_sent_entry: Mutex<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>,

    phantom: PhantomData<Fingerprint>,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        PD,
        AT,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        Store,
        StoreCreationFunction,
        Fingerprint,
    >
    ReconciliationSender<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        PD,
        AT,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        Store,
        StoreCreationFunction,
        Fingerprint,
    >
where
    S: Default,
{
    pub(crate) fn new(
        pio_state: Rc<
            pio::State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                MyReadCap,
                MyEnumCap,
                P,
                PFinal,
                PErr,
                C,
                CErr,
            >,
        >,
        storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
        reconciliation_channel_sender: ChannelSender<5, P, PFinal, PErr, C, CErr>,
        data_channel_sender: Rc<Mutex<ChannelSender<5, P, PFinal, PErr, C, CErr>>>,
        session_id: u64,
        default_authorised_entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    ) -> Self {
        Self {
            pio_state,
            storedinator,
            reconciliation_channel_sender: Mutex::new(reconciliation_channel_sender),
            data_channel_sender,
            session_id,
            previously_sent_fingerprint_range: Mutex::new(Range3d::default()),
            previously_sent_itemset_range: Mutex::new(Range3d::default()),
            previously_sent_entry: Mutex::new(default_authorised_entry),
            phantom: PhantomData,
        }
    }
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        PD,
        AT,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        Store,
        StoreCreationFunction,
        StoreCreationError,
        FP,
    >
    ReconciliationSender<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        PD,
        AT,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        Store,
        StoreCreationFunction,
        FP,
    >
where
    StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
    Store: RbsrStore<MCL, MCC, MPL, N, S, PD, AT, FP>,
    N: WgpsNamespaceId,
    S: WgpsSubspaceId,
    PD: WgpsPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    C: BulkConsumer<Item = u8, Final = (), Error = CErr>,
    CErr: Clone,
    FP: WgpsFingerprint<MCL, MCC, MPL, N, S, PD, AT>,
{
    /// Sends messages depending on a split action. After sending an item set, returns a store subscription to the 3d range of that item set.
    pub(crate) async fn process_split_action(
        &self,
        namespace_id: &N,
        range: &Range3d<MCL, MCC, MPL, S>,
        split_action_to_process: SplitAction<FP>,
        // Used in the messages this sends, no need to do any specific processing of it. That happened elsewhere already.
        root_id: u64,
        sender_handle: u64,
        receiver_handle: u64,
    ) -> Result<
        Option<(
            impl Producer<
                Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
                Final = (),
                Error = Store::Error,
            >,
            SubscriptionMetadata<MCL, MCC, MPL, N, S>,
        )>,
        RbsrError<CErr, StoreCreationError, Store::Error>,
    > {
        match split_action_to_process {
            SplitAction::SendFingerprint(fp_to_send) => {
                let msg = ReconciliationSendFingerprint {
                    fingerprint: fp_to_send,
                    info: RangeInfo {
                        root_id: root_id,
                        range: range.clone(),
                        sender_handle,
                        receiver_handle,
                    },
                };

                let mut prev_range = self.previously_sent_fingerprint_range.write().await;

                self.reconciliation_channel_sender
                    .write()
                    .await
                    .send_to_channel_relative(&msg, prev_range.deref())
                    .await
                    .map_err(
                        |logical_channel_client_error| match logical_channel_client_error {
                            LogicalChannelClientError::Underlying(inner_err) => {
                                RbsrError::Sending(inner_err)
                            }
                            LogicalChannelClientError::LogicalChannelClosed => {
                                RbsrError::ReconciliationChannelClosedByPeer
                            }
                        },
                    )?;

                *prev_range = range.clone();

                return Ok(None);
            }
            SplitAction::SendEntries(_approximate_count) => {
                return Ok(Some(
                    self.send_entries_in_range(
                        namespace_id,
                        range,
                        root_id,
                        sender_handle,
                        receiver_handle,
                        None,
                    )
                    .await?,
                ));
            }
        }
    }

    pub async fn send_entries_in_range(
        &self,
        namespace_id: &N,
        range: &Range3d<MCL, MCC, MPL, S>,
        // Used in the messages this sends, no need to do any specific processing of it. That happened elsewhere already.
        root_id: u64,
        sender_handle: u64,
        receiver_handle: u64,
        sorted_entries_to_skip: Option<&[LengthyEntry<MCL, MCC, MPL, N, S, PD>]>,
    ) -> Result<
        (
            impl Producer<
                Item = StoreEvent<MCL, MCC, MPL, N, S, PD, AT>,
                Final = (),
                Error = Store::Error,
            >,
            SubscriptionMetadata<MCL, MCC, MPL, N, S>,
        ),
        RbsrError<CErr, StoreCreationError, Store::Error>,
    > {
        let subscription_metadata = SubscriptionMetadata {
            root_id,
            namespace_id: namespace_id.clone(),
            range: range.clone(),
        };

        let store = self
            .storedinator
            .get_store(namespace_id)
            .await
            .map_err(|store_creation_error| RbsrError::StoreCreation(store_creation_error))?;

        let mut entries = store
            .query_and_subscribe_range(range.clone(), QueryIgnoreParams::default())
            .await?;

        let mut current_entry = loop {
            match entries.produce().await? {
                Left(entry) => match sorted_entries_to_skip {
                    None => break entry,
                    Some(entries) => {
                        if entries
                            .binary_search_by(|entry_in_slice| {
                                order_entries(
                                    entry_in_slice,
                                    &LengthyEntry::new(
                                        entry.entry().entry().clone(),
                                        entry.available(),
                                    ),
                                )
                            })
                            .is_ok()
                        {
                            continue;
                        } else {
                            break entry;
                        }
                    }
                },
                Right(subscription) => {
                    // Send a ReconciliationAnnounceEntries message with `is_empty = true`, update `self.previously_sent_range`, and return the store subscription for the range.
                    let msg = ReconciliationAnnounceEntries {
                        info: RangeInfo {
                            root_id: root_id,
                            range: range.clone(),
                            sender_handle,
                            receiver_handle,
                        },
                        is_empty: true,
                        want_response: sorted_entries_to_skip.is_none(),
                        will_sort: false,
                    };

                    let mut prev_range = self.previously_sent_itemset_range.write().await;

                    self.data_channel_sender
                        .write()
                        .await
                        .send_to_channel_relative(&msg, prev_range.deref())
                        .await
                        .map_err(
                            |logical_channel_client_error| match logical_channel_client_error {
                                LogicalChannelClientError::Underlying(inner_err) => {
                                    RbsrError::Sending(inner_err)
                                }
                                LogicalChannelClientError::LogicalChannelClosed => {
                                    RbsrError::DataChannelClosedByPeer
                                }
                            },
                        )?;

                    *prev_range = range.clone();

                    return Ok((subscription, subscription_metadata));
                }
            }
        };

        let mut next_entry_or_subscription = entries.produce().await?;

        // Send a ReconciliationAnnounceEntries message with `is_empty = false`, update `self.previously_sent_range`, and then go into a loop to send all entries in the range
        let announce_entries_msg = ReconciliationAnnounceEntries {
            info: RangeInfo {
                root_id: root_id,
                range: range.clone(),
                sender_handle,
                receiver_handle,
            },
            is_empty: false,
            want_response: sorted_entries_to_skip.is_none(),
            will_sort: false,
        };

        let mut prev_range = self.previously_sent_itemset_range.write().await;

        self.data_channel_sender
            .write()
            .await
            .send_to_channel_relative(&announce_entries_msg, prev_range.deref())
            .await
            .map_err(
                |logical_channel_client_error| match logical_channel_client_error {
                    LogicalChannelClientError::Underlying(inner_err) => {
                        RbsrError::Sending(inner_err)
                    }
                    LogicalChannelClientError::LogicalChannelClosed => {
                        RbsrError::DataChannelClosedByPeer
                    }
                },
            )?;

        *prev_range = range.clone();

        loop {
            let send_entry_msg = ReconciliationSendEntry {
                entry: current_entry.clone(),
                offset: 0,
            };

            let mut prev_entry = self.previously_sent_entry.write().await;

            self.data_channel_sender
                .write()
                .await
                .send_to_channel_relative(
                    &send_entry_msg,
                    &(prev_range.deref(), prev_entry.deref()),
                )
                .await
                .map_err(
                    |logical_channel_client_error| match logical_channel_client_error {
                        LogicalChannelClientError::Underlying(inner_err) => {
                            RbsrError::Sending(inner_err)
                        }
                        LogicalChannelClientError::LogicalChannelClosed => {
                            RbsrError::DataChannelClosedByPeer
                        }
                    },
                )?;

            *prev_entry = current_entry.into_authorised_entry();

            let terminate_payload_msg = ReconciliationTerminatePayload {
                is_final: next_entry_or_subscription.is_right(),
            };

            self.data_channel_sender
                .write()
                .await
                .send_to_channel(&terminate_payload_msg)
                .await
                .map_err(
                    |logical_channel_client_error| match logical_channel_client_error {
                        LogicalChannelClientError::Underlying(inner_err) => {
                            RbsrError::Sending(inner_err)
                        }
                        LogicalChannelClientError::LogicalChannelClosed => {
                            RbsrError::DataChannelClosedByPeer
                        }
                    },
                )?;

            loop {
                match next_entry_or_subscription {
                    Left(next_entry) => {
                        current_entry = next_entry;
                        next_entry_or_subscription = entries.produce().await?;

                        match sorted_entries_to_skip {
                            None => {
                                // break the inner loop, thus processing current_entry
                                break;
                            }
                            Some(entries) => {
                                if entries
                                    .binary_search_by(|entry_in_slice| {
                                        order_entries(
                                            entry_in_slice,
                                            &LengthyEntry::new(
                                                current_entry.entry().entry().clone(),
                                                current_entry.available(),
                                            ),
                                        )
                                    })
                                    .is_ok()
                                {
                                    // do nothing else, go to next iteration of this inner loop, thus skipping the current_entry
                                } else {
                                    // break the inner loop, thus processing current_entry
                                    break;
                                }
                            }
                        }
                    }
                    Right(subscription) => {
                        return Ok((subscription, subscription_metadata));
                    }
                }
            }
        }
    }
}

fn order_entries<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>(
    a: &LengthyEntry<MCL, MCC, MPL, N, S, PD>,
    b: &LengthyEntry<MCL, MCC, MPL, N, S, PD>,
) -> core::cmp::Ordering
where
    S: Ord,
{
    a.entry()
        .subspace_id()
        .cmp(b.entry().subspace_id())
        .then_with(|| a.entry().path().cmp(b.entry().path()))
        .then_with(|| a.available().cmp(&b.available()))
}

pub(crate) struct SubscriptionMetadata<const MCL: usize, const MCC: usize, const MPL: usize, N, S> {
    root_id: u64,
    namespace_id: N,
    range: Range3d<MCL, MCC, MPL, S>,
}

pub(crate) enum RbsrError<SendingError, StoreCreationError, StoreError> {
    Sending(SendingError),
    StoreCreation(StoreCreationError),
    Store(StoreError),
    ReconciliationChannelClosedByPeer,
    DataChannelClosedByPeer,
}

impl<SendingError, StoreCreationError, StoreErr> From<StoreErr>
    for RbsrError<SendingError, StoreCreationError, StoreErr>
{
    fn from(value: StoreErr) -> Self {
        RbsrError::Store(value)
    }
}

// /// Use this struct to process all incoming messages on the reconciliation channel (via the `process_incoming_reconciliation_messages` method).
// pub(crate) struct ReconciliationReceiver<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     MyReadCap,
//     MyEnumCap,
//     P,
//     PFinal,
//     PErr,
//     C,
//     CErr,
//     Store,
//     StoreCreationFunction,
//     Fingerprint,
//     PD,
//     AT,
// > {
//     state: Rc<
//         ReconciliationSender<
//             SALT_LENGTH,
//             INTEREST_HASH_LENGTH,
//             MCL,
//             MCC,
//             MPL,
//             N,
//             S,
//             MyReadCap,
//             MyEnumCap,
//             P,
//             PFinal,
//             PErr,
//             C,
//             CErr,
//             Store,
//             StoreCreationFunction,
//             Fingerprint,
//             PD,
//             AT,
//         >,
//     >,
//     receiver: ChannelReceiver<5, P, PFinal, PErr, C, CErr>,
//     phantom: PhantomData<(Fingerprint, PD, AT)>,
//     received_entry_buffer: Vec<Entry<MCL, MCC, MPL, N, S, PD>>,
//     received_entry_buffer_capacity: usize,
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         StoreCreationError,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationReceiver<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// where
//     N: Eq + core::hash::Hash + Clone,
//     S: Clone + Ord,
//     PD: Clone,
//     AT: Clone,
//     Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
//     StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
// {
//     pub async fn process_incoming_reconciliation_messages(
//         &mut self,
//     ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
//         loop {
//             match self.decode_send_fp_or_announce_entries_msg().await? {
//                 Left(send_fp_msg) => {
//                     todo!("act on fingerprint message");
//                     continue; // go to next iteration of decoding loop
//                 }
//                 Right(announce_entries_msg) => {
//                     let mut received_all_messages_for_this_range = announce_entries_msg.is_empty;
//                     self.received_entry_buffer.clear();

//                     if received_all_messages_for_this_range {
//                         todo!("process announce_entries_msg.info.root_id");
//                         todo!("this is only inside an if-statement to get rid of dead-code warnings; do this unconditionally");
//                     }

//                     // Decode entries and their payloads until we got everything in the range
//                     while !received_all_messages_for_this_range {
//                         // Decode entries and their payload until a `ReconciliationTerminatePayload` msg with `is_final`
//                         let send_entry_msg = self.decode_send_entry_msg_and_verify().await?;
//                         let entry = send_entry_msg.entry.entry().entry();

//                         let store = self
//                             .state
//                             .storedinator
//                             .get_store(entry.namespace_id())
//                             .await
//                             .map_err(|store_creation_error| {
//                                 RbsrError::StoreCreation(store_creation_error)
//                             })?;

//                         match store
//                             .ingest_entry(
//                                 send_entry_msg.entry.entry().clone(),
//                                 false,
//                                 EntryOrigin::Remote(self.state.session_id),
//                             )
//                             .await {
//                                 Ok(_) => {/* no-op */}
//                                 Err(EntryIngestionError::PruningPrevented) => panic!("Got a EntryIngestionError::PruningPrevented error despite calling Store::ingest_entry with prevent_pruning set to false. The store implementation is buggy."),
//                                 Err(EntryIngestionError::OperationsError(store_err)) => return Err(RbsrError::Store(store_err)),
//                             }

//                         if self.received_entry_buffer.len() < self.received_entry_buffer_capacity {
//                             self.received_entry_buffer
//                                 .push(send_entry_msg.entry.entry().entry().clone());
//                         }

//                         // This flag ensures that receiving the payload for the same entry concurrently form multiple peers doesn't result in storing an incorrect payload.
//                         let mut try_appending_payload = true;

//                         loop {
//                             match self.decode_send_payload_or_terminate_payload_msg().await? {
//                                 Left(send_payload_msg) => {
//                                     if try_appending_payload {
//                                         let mut bytes_to_append =
//                                             FromSlice::new(&send_payload_msg.bytes[..]);
//                                         match store
//                                             .append_payload(
//                                                 entry.subspace_id(),
//                                                 entry.path(),
//                                                 Some(entry.payload_digest().clone()),
//                                                 Some(todo!()),
//                                                 &mut bytes_to_append,
//                                             )
//                                             .await
//                                         {
//                                             Ok(_) => { /* no-op */ }
//                                             Err(PayloadAppendError::OperationError(store_err)) => {
//                                                 return Err(RbsrError::from(store_err));
//                                             }
//                                             Err(_) => {
//                                                 try_appending_payload = false;
//                                             }
//                                         }
//                                     }

//                                     // nothing more to do, go to next iteration of the (inner)ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
//                                 }
//                                 Right(terminate_payload_msg) => {
//                                     received_all_messages_for_this_range =
//                                         terminate_payload_msg.is_final;
//                                     break; // leaves the (inner) ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
//                                 }
//                             }
//                         }

//                         // Sort buffered received entries so we can query more efficiently whether a given entry is currently buffered.
//                         self.received_entry_buffer.sort_by(sort_entries);

//                         self.state
//                             .send_my_entries_in_range(
//                                 &announce_entries_msg.info.range,
//                                 &self.received_entry_buffer[..],
//                             )
//                             .await?;

//                         // All done with this ReconciliationAnnounceEntries message, move on to the next iteration of the outer decoding loop.
//                     }
//                 }
//             }
//         }
//     }

//     async fn decode_send_fp_or_announce_entries_msg(
//         &mut self,
//     ) -> Result<
//         Either<
//             ReconciliationSendFingerprint<MCL, MCC, MPL, S, Fingerprint>,
//             ReconciliationAnnounceEntries<MCL, MCC, MPL, S>,
//         >,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!()
//     }

//     async fn decode_send_entry_msg_and_verify(
//         &mut self,
//     ) -> Result<
//         ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!("verify message (resource handles) and (?) the entry itself");
//     }

//     async fn decode_send_payload_or_terminate_payload_msg(
//         &mut self,
//     ) -> Result<
//         Either<ReconciliationSendPayload, ReconciliationTerminatePayload>,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!()
//     }
// }

//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//

// use std::marker::PhantomData;
// use std::rc::Rc;

// use either::Either::{self, Left, Right};
// use lcmux::{ChannelReceiver, ChannelSender};
// use ufotofu::producer::FromSlice;
// use wb_async_utils::Mutex;
// use willow_data_model::grouping::Range3d;
// use willow_data_model::{
//     Component, Entry, EntryIngestionError, EntryOrigin, LengthyAuthorisedEntry, Path, PathBuilder,
//     PayloadAppendError,
// };

// use crate::messages::{
//     ReconciliationAnnounceEntries, ReconciliationSendEntry, ReconciliationSendFingerprint,
//     ReconciliationSendPayload, ReconciliationTerminatePayload,
// };
// use crate::pio::overlap_finder::NamespacedAoIWithMaxPayloadPower;
// use crate::{pio, storedinator::Storedinator};

// /// The state per WGPS session for handling RBSR.
// struct ReconciliationState<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     MyReadCap,
//     MyEnumCap,
//     P,
//     PFinal,
//     PErr,
//     C,
//     CErr,
//     Store,
//     StoreCreationFunction,
//     Fingerprint,
//     PD,
//     AT,
// > {
//     /// Need this to decode messages and to check validity of incoming messages (whether their capability-handle pairs are valid and refer to overlapping areas).
//     pio_state: Rc<
//         pio::State<
//             SALT_LENGTH,
//             INTEREST_HASH_LENGTH,
//             MCL,
//             MCC,
//             MPL,
//             N,
//             S,
//             MyReadCap,
//             MyEnumCap,
//             P,
//             PFinal,
//             PErr,
//             C,
//             CErr,
//         >,
//     >,
//     /// Access to our stores.
//     storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
//     reconciliation_channel_sender: Mutex<ChannelSender<4, P, PFinal, PErr, C, CErr>>,
//     session_id: u64,

//     // temp
//     todoRemoveThis: PhantomData<(Fingerprint, PD, AT)>,
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationState<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// {
//     fn new(
//         pio_state: Rc<
//             pio::State<
//                 SALT_LENGTH,
//                 INTEREST_HASH_LENGTH,
//                 MCL,
//                 MCC,
//                 MPL,
//                 N,
//                 S,
//                 MyReadCap,
//                 MyEnumCap,
//                 P,
//                 PFinal,
//                 PErr,
//                 C,
//                 CErr,
//             >,
//         >,
//         storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
//         reconciliation_channel_sender: Mutex<ChannelSender<4, P, PFinal, PErr, C, CErr>>,
//         session_id: u64,
//     ) -> Self {
//         Self {
//             pio_state,
//             storedinator,
//             reconciliation_channel_sender,
//             session_id,
//             todoRemoveThis: PhantomData,
//         }
//     }
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         StoreCreationError,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationState<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// where
//     StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
//     Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
// {
//     async fn send_my_entries_in_range(
//         &self,
//         range: &Range3d<MCL, MCC, MPL, S>,
//         // Must be sorted (subspace_id first, lexicographic path as tiebreaker)
//         except_for: &[Entry<MCL, MCC, MPL, N, S, PD>],
//     ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
//         todo!();
//     }
// }

// /// The entrypoint to reconciliatoin (see the associated `new` function).
// pub(crate) struct ReconciliationSession<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     MyReadCap,
//     MyEnumCap,
//     P,
//     PFinal,
//     PErr,
//     C,
//     CErr,
//     Store,
//     StoreCreationFunction,
//     Fingerprint,
//     PD,
//     AT,
// > {
//     /// Use this value to initiate reconciliation for detected overlaps (only if you initiated the reconciliation session, otherwise ignore overlaps).
//     pub initiator: ReconciliationInitiator<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >,
//     /// Use this value to process all incoming messages on the reconciliation channel (via the `process_incoming_reconciliation_messages` method).
//     pub receiver: ReconciliationReceiver<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >,
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationSession<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// {
//     /// The entrypoint to reconciliation. Use the public fields of this struct for the separate sub-task.
//     ///
//     /// `entry_buffer_capacity` is the max number of entries to buffer in memory while receiving `ReconciliationSendEntry` messages and detecting duplicates to filter which entries to send in reply.
//     ///
//     /// `session_id` is an arbitrary numeric identifier that must differ from the id of all other reconciliation sessions. It is used internally to ensure that we do not echo the entries we receive from a peer back to them.
//     pub fn new(
//         pio_state: Rc<
//             pio::State<
//                 SALT_LENGTH,
//                 INTEREST_HASH_LENGTH,
//                 MCL,
//                 MCC,
//                 MPL,
//                 N,
//                 S,
//                 MyReadCap,
//                 MyEnumCap,
//                 P,
//                 PFinal,
//                 PErr,
//                 C,
//                 CErr,
//             >,
//         >,
//         storedinator: Rc<Storedinator<Store, StoreCreationFunction, N>>,
//         reconciliation_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
//         reconciliation_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
//         entry_buffer_capacity: usize,
//         session_id: u64,
//     ) -> Self {
//         let state = Rc::new(ReconciliationState::new(
//             pio_state,
//             storedinator,
//             Mutex::new(reconciliation_channel_sender),
//             session_id,
//         ));

//         Self {
//             initiator: ReconciliationInitiator {
//                 state: state.clone(),
//             },
//             receiver: ReconciliationReceiver {
//                 state: state,
//                 receiver: reconciliation_channel_receiver,
//                 phantom: PhantomData,
//                 received_entry_buffer: Vec::with_capacity(entry_buffer_capacity),
//                 received_entry_buffer_capacity: entry_buffer_capacity,
//             },
//         }
//     }
// }

// /// Use this struct to initiate reconciliation for detected overlaps (only if you initiated the reconciliation session, otherwise ignore overlaps).
// pub(crate) struct ReconciliationInitiator<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     MyReadCap,
//     MyEnumCap,
//     P,
//     PFinal,
//     PErr,
//     C,
//     CErr,
//     Store,
//     StoreCreationFunction,
//     Fingerprint,
//     PD,
//     AT,
// > {
//     state: Rc<
//         ReconciliationState<
//             SALT_LENGTH,
//             INTEREST_HASH_LENGTH,
//             MCL,
//             MCC,
//             MPL,
//             N,
//             S,
//             MyReadCap,
//             MyEnumCap,
//             P,
//             PFinal,
//             PErr,
//             C,
//             CErr,
//             Store,
//             StoreCreationFunction,
//             Fingerprint,
//             PD,
//             AT,
//         >,
//     >,
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         StoreCreationError,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationInitiator<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// where
//     StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
//     Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
// {
//     /// Call this when we detected an overlap in PIO (and are the initiator of the WGPS session). This then sends the necessary message(s) for initiating RBSR on the overlap.
//     pub async fn initiate_reconciliation(
//         &mut self,
//         overlap: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
//     ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
//         todo!("implement this")
//     }
// }

// /// Use this struct to process all incoming messages on the reconciliation channel (via the `process_incoming_reconciliation_messages` method).
// pub(crate) struct ReconciliationReceiver<
//     const SALT_LENGTH: usize,
//     const INTEREST_HASH_LENGTH: usize,
//     const MCL: usize,
//     const MCC: usize,
//     const MPL: usize,
//     N,
//     S,
//     MyReadCap,
//     MyEnumCap,
//     P,
//     PFinal,
//     PErr,
//     C,
//     CErr,
//     Store,
//     StoreCreationFunction,
//     Fingerprint,
//     PD,
//     AT,
// > {
//     state: Rc<
//         ReconciliationState<
//             SALT_LENGTH,
//             INTEREST_HASH_LENGTH,
//             MCL,
//             MCC,
//             MPL,
//             N,
//             S,
//             MyReadCap,
//             MyEnumCap,
//             P,
//             PFinal,
//             PErr,
//             C,
//             CErr,
//             Store,
//             StoreCreationFunction,
//             Fingerprint,
//             PD,
//             AT,
//         >,
//     >,
//     receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
//     phantom: PhantomData<(Fingerprint, PD, AT)>,
//     received_entry_buffer: Vec<Entry<MCL, MCC, MPL, N, S, PD>>,
//     received_entry_buffer_capacity: usize,
// }

// impl<
//         const SALT_LENGTH: usize,
//         const INTEREST_HASH_LENGTH: usize,
//         const MCL: usize,
//         const MCC: usize,
//         const MPL: usize,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         StoreCreationError,
//         Fingerprint,
//         PD,
//         AT,
//     >
//     ReconciliationReceiver<
//         SALT_LENGTH,
//         INTEREST_HASH_LENGTH,
//         MCL,
//         MCC,
//         MPL,
//         N,
//         S,
//         MyReadCap,
//         MyEnumCap,
//         P,
//         PFinal,
//         PErr,
//         C,
//         CErr,
//         Store,
//         StoreCreationFunction,
//         Fingerprint,
//         PD,
//         AT,
//     >
// where
//     N: Eq + core::hash::Hash + Clone,
//     S: Clone + Ord,
//     PD: Clone,
//     AT: Clone,
//     Store: willow_data_model::Store<MCL, MCC, MPL, N, S, PD, AT>,
//     StoreCreationFunction: AsyncFn(&N) -> Result<Store, StoreCreationError>,
// {
//     pub async fn process_incoming_reconciliation_messages(
//         &mut self,
//     ) -> Result<(), RbsrError<CErr, StoreCreationError, Store::Error>> {
//         loop {
//             match self.decode_send_fp_or_announce_entries_msg().await? {
//                 Left(send_fp_msg) => {
//                     todo!("act on fingerprint message");
//                     continue; // go to next iteration of decoding loop
//                 }
//                 Right(announce_entries_msg) => {
//                     let mut received_all_messages_for_this_range = announce_entries_msg.is_empty;
//                     self.received_entry_buffer.clear();

//                     if received_all_messages_for_this_range {
//                         todo!("process announce_entries_msg.info.root_id");
//                         todo!("this is only inside an if-statement to get rid of dead-code warnings; do this unconditionally");
//                     }

//                     // Decode entries and their payloads until we got everything in the range
//                     while !received_all_messages_for_this_range {
//                         // Decode entries and their payload until a `ReconciliationTerminatePayload` msg with `is_final`
//                         let send_entry_msg = self.decode_send_entry_msg_and_verify().await?;
//                         let entry = send_entry_msg.entry.entry().entry();

//                         let store = self
//                             .state
//                             .storedinator
//                             .get_store(entry.namespace_id())
//                             .await
//                             .map_err(|store_creation_error| {
//                                 RbsrError::StoreCreation(store_creation_error)
//                             })?;

//                         match store
//                             .ingest_entry(
//                                 send_entry_msg.entry.entry().clone(),
//                                 false,
//                                 EntryOrigin::Remote(self.state.session_id),
//                             )
//                             .await {
//                                 Ok(_) => {/* no-op */}
//                                 Err(EntryIngestionError::PruningPrevented) => panic!("Got a EntryIngestionError::PruningPrevented error despite calling Store::ingest_entry with prevent_pruning set to false. The store implementation is buggy."),
//                                 Err(EntryIngestionError::OperationsError(store_err)) => return Err(RbsrError::Store(store_err)),
//                             }

//                         if self.received_entry_buffer.len() < self.received_entry_buffer_capacity {
//                             self.received_entry_buffer
//                                 .push(send_entry_msg.entry.entry().entry().clone());
//                         }

//                         // This flag ensures that receiving the payload for the same entry concurrently form multiple peers doesn't result in storing an incorrect payload.
//                         let mut try_appending_payload = true;

//                         loop {
//                             match self.decode_send_payload_or_terminate_payload_msg().await? {
//                                 Left(send_payload_msg) => {
//                                     if try_appending_payload {
//                                         let mut bytes_to_append =
//                                             FromSlice::new(&send_payload_msg.bytes[..]);
//                                         match store
//                                             .append_payload(
//                                                 entry.subspace_id(),
//                                                 entry.path(),
//                                                 Some(entry.payload_digest().clone()),
//                                                 Some(todo!()),
//                                                 &mut bytes_to_append,
//                                             )
//                                             .await
//                                         {
//                                             Ok(_) => { /* no-op */ }
//                                             Err(PayloadAppendError::OperationError(store_err)) => {
//                                                 return Err(RbsrError::from(store_err));
//                                             }
//                                             Err(_) => {
//                                                 try_appending_payload = false;
//                                             }
//                                         }
//                                     }

//                                     // nothing more to do, go to next iteration of the (inner)ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
//                                 }
//                                 Right(terminate_payload_msg) => {
//                                     received_all_messages_for_this_range =
//                                         terminate_payload_msg.is_final;
//                                     break; // leaves the (inner) ReconciliationSendPayload-or-ReconciliationTerminatePayload loop
//                                 }
//                             }
//                         }

//                         // Sort buffered received entries so we can query more efficiently whether a given entry is currently buffered.
//                         self.received_entry_buffer.sort_by(sort_entries);

//                         self.state
//                             .send_my_entries_in_range(
//                                 &announce_entries_msg.info.range,
//                                 &self.received_entry_buffer[..],
//                             )
//                             .await?;

//                         // All done with this ReconciliationAnnounceEntries message, move on to the next iteration of the outer decoding loop.
//                     }
//                 }
//             }
//         }
//     }

//     async fn decode_send_fp_or_announce_entries_msg(
//         &mut self,
//     ) -> Result<
//         Either<
//             ReconciliationSendFingerprint<MCL, MCC, MPL, S, Fingerprint>,
//             ReconciliationAnnounceEntries<MCL, MCC, MPL, S>,
//         >,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!()
//     }

//     async fn decode_send_entry_msg_and_verify(
//         &mut self,
//     ) -> Result<
//         ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!("verify message (resource handles) and (?) the entry itself");
//     }

//     async fn decode_send_payload_or_terminate_payload_msg(
//         &mut self,
//     ) -> Result<
//         Either<ReconciliationSendPayload, ReconciliationTerminatePayload>,
//         RbsrError<CErr, StoreCreationError, Store::Error>,
//     > {
//         todo!()
//     }
// }

// pub(crate) enum RbsrError<SendingError, StoreCreationError, StoreErr> {
//     Sending(SendingError),
//     StoreCreation(StoreCreationError),
//     Store(StoreErr),
// }

// impl<SendingError, StoreCreationError, StoreErr> From<StoreErr>
//     for RbsrError<SendingError, StoreCreationError, StoreErr>
// {
//     fn from(value: StoreErr) -> Self {
//         RbsrError::Store(value)
//     }
// }

// fn sort_entries<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>(
//     a: &Entry<MCL, MCC, MPL, N, S, PD>,
//     b: &Entry<MCL, MCC, MPL, N, S, PD>,
// ) -> core::cmp::Ordering
// where
//     S: Ord,
// {
//     a.subspace_id()
//         .cmp(b.subspace_id())
//         .then_with(|| a.path().cmp(b.path()))
// }
