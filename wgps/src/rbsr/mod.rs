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
use crate::parameters::{
    WgpsAuthorisationToken, WgpsFingerprint, WgpsNamespaceId, WgpsPayloadDigest, WgpsSubspaceId,
};
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
    AT: WgpsAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
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
                    &((namespace_id, prev_range.deref()), prev_entry.deref()),
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

pub(crate) fn order_entries<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>(
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
    pub root_id: u64,
    pub namespace_id: N,
    pub range: Range3d<MCL, MCC, MPL, S>,
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
