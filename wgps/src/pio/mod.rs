use std::{borrow::Borrow, cell::RefCell, collections::HashSet, hash::Hash, ops::Deref};

use compact_u64::CompactU64;
use either::Either::{Left, Right};
use lcmux::{ChannelReceiver, ChannelSender};
use ufotofu::{
    producer::{MapItem, Merge},
    BulkConsumer, Consumer, Producer,
};
use ufotofu_codec::{
    adaptors::{Decoder, RelativeDecoder},
    Blame, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
    RelativeEncodableKnownSize,
};
use wb_async_utils::{
    rw::WriteGuard,
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    Mutex, RwLock,
};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    NamespaceId, Path, SubspaceId,
};
use willow_pio::{PersonalPrivateInterest, PrivateInterest};

use crate::{
    messages::{PioAnnounceOverlap, PioBindHash, PioBindReadCapability},
    pio::{
        capable_aois::{InterestRegistry, ReceivedAnnouncementError, ReceivedBindCapabilityError},
        salted_hashes::Overlap,
    },
};

mod capable_aois;
mod salted_hashes;

/// The state for a PIO session.
///
/// This struct is opaque, but we expose it to allow for control over where it is allocated.
#[derive(Debug)]
pub struct State<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    EnumCap,
    P,
    PR,
    C,
    CR,
    LcmuxStateRef,
> where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    p: SharedProducer<PR, P>,
    c: SharedConsumer<CR, C>,
    interest_registry: RwLock<
        InterestRegistry<SALT_LENGTH, INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap>,
    >,
    caois_for_which_we_already_sent_a_bind_read_capability_message:
        RefCell<HashSet<CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>>>,
    capability_channel_sender: Mutex<ChannelSender<4, LcmuxStateRef, P, PR, C, CR>>,
    my_public_key: S,
    their_public_key: S,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        LcmuxStateRef,
    >
    State<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        LcmuxStateRef,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S> + Eq + Hash,
    EnumCap: Eq + Hash,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    /// Create a new opaque state for a PIO session.
    pub fn new(
        p: SharedProducer<PR, P>,
        c: SharedConsumer<CR, C>,
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
        my_public_key: S,
        their_public_key: S,
        capability_channel_sender: ChannelSender<4, LcmuxStateRef, P, PR, C, CR>,
    ) -> Self {
        Self {
            p,
            c,
            interest_registry: RwLock::new(InterestRegistry::new(my_salt, h)),
            caois_for_which_we_already_sent_a_bind_read_capability_message: RefCell::new(
                HashSet::new(),
            ),
            capability_channel_sender: Mutex::new(capability_channel_sender),
            my_public_key,
            their_public_key,
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
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        LcmuxStateRef,
    >
    State<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        LcmuxStateRef,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Eq
        + Hash
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
    EnumCap: Eq
        + Hash
        + Clone
        + EnumerationCapability<Receiver = S, NamespaceId = N>
        + RelativeEncodableKnownSize<(N, S)>,
    P: Producer,
    C: Consumer<Item = u8> + BulkConsumer,
    C::Final: Clone,
    C::Error: Clone,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>>,
{
    // Do everything you need to do when our internal APIs signal an Overlap (i.e., send PioAnnounceOverlap and PioBindReadCapability messages).
    async fn process_overlap<'lock>(
        &self,
        overlap: &Overlap,
        interest_registry: &mut WriteGuard<
            'lock,
            InterestRegistry<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
            >,
        >,
    ) -> Result<(), ProcessOverlapError<C::Error>> {
        let caois =
            interest_registry.caois_for_my_interesting_handle(overlap.my_interesting_handle);
        let private_interest = interest_registry
            .private_interest_for_my_interesting_handle(overlap.my_interesting_handle);

        if overlap.should_send_capability {
            let ppi = PersonalPrivateInterest {
                private_interest: private_interest.clone(),
                user_key: self.their_public_key.clone(),
            };

            for caoi in caois {
                let should_skip = self
                    .caois_for_which_we_already_sent_a_bind_read_capability_message
                    .borrow_mut()
                    .contains(caoi);
                if should_skip {
                    continue;
                } else {
                    let msg = PioBindReadCapability {
                        sender_handle: overlap.my_interesting_handle,
                        receiver_handle: overlap.their_handle,
                        max_count: caoi.max_count,
                        max_size: caoi.max_size,
                        max_payload_power: caoi.max_payload_power,
                        capability: caoi.capability.clone(),
                    };

                    self.capability_channel_sender
                        .write()
                        .await
                        .send_to_channel_relative(&msg, &ppi)
                        .await?;

                    self.caois_for_which_we_already_sent_a_bind_read_capability_message
                        .borrow_mut()
                        .insert(caoi.clone());
                }
            }

            return Ok(());
        } else {
            debug_assert!(overlap.should_request_capability);

            let pair = (
                private_interest.namespace_id().clone(),
                self.my_public_key.clone(),
            );

            for caoi in caois {
                let enumeration_capability = if overlap.awkward {
                    Some(
                        caoi.enum_cap
                            .clone()
                            .ok_or(ProcessOverlapError::NoEnumerationCapability)?,
                    )
                } else {
                    None
                };

                let authentication = (interest_registry.hash_registry.h)(
                    private_interest,
                    &interest_registry.hash_registry.my_salt,
                );

                let msg: PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap> = PioAnnounceOverlap {
                    sender_handle: overlap.my_interesting_handle,
                    receiver_handle: overlap.their_handle,
                    authentication,
                    enumeration_capability,
                };

                self.capability_channel_sender
                    .write()
                    .await
                    .send_to_channel_relative(&msg, &pair)
                    .await?;
            }

            Ok(())
        }
    }
}

/// Everything that makes up a single pio session.
pub struct PioSession<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash + 'static,
    S: SubspaceId + Hash + 'static,
    ReadCap,
    EnumCap: 'static,
    P,
    PR,
    C,
    CR,
    StateRef: Deref<
            Target = State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + Borrow<
            State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + 'static,
    LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>> + 'static,
    AnnounceOverlapProducer,
> where
    P: Producer + 'static,
    C: Consumer + 'static,
    PR: Deref<Target = shared_producer::State<P>> + Clone + 'static,
    CR: Deref<Target = shared_consumer::State<C>> + Clone + 'static,
    ReadCap: ReadCapability<MCL, MCC, MPL>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
{
    pub my_aoi_input: MyAoiInput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
    >,
    pub overlap_output: OverlappingAoiOutput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
        AnnounceOverlapProducer,
    >,
    // pub capability_handles: AoiHandles,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Hash + 'static,
        S: SubspaceId + Hash + 'static,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
        AnnounceOverlapProducer,
    >
    PioSession<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
        AnnounceOverlapProducer,
    >
where
    P: Producer + 'static,
    C: BulkConsumer<Item = u8, Final: Clone, Error: Clone> + 'static,
    PR: Deref<Target = shared_producer::State<P>> + Clone + 'static,
    CR: Deref<Target = shared_consumer::State<C>> + Clone + 'static,
    StateRef: Deref<
            Target = State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + Borrow<
            State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + Clone
        + 'static,
    LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>>,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
        + 'static,
    EnumCap: Clone
        + Hash
        + Eq
        + 'static
        + RelativeEncodableKnownSize<(N, S)>
        + EnumerationCapability<NamespaceId = N, Receiver = S>,
{
    /// This function is the entry point to our implementation of private interest overlap detection.
    ///
    /// While the internals are somewhat complex, the outer interface of PIO is fairly simple: peers exchange PioBindReadCapability messages. Each of these is essentially an AreaOfInterest together with a read capability for that AoI.
    ///
    /// This function returns:
    ///
    /// - a [`MyAoiInput`], where you can put in the AoIs you'd like to sync (with backpressure applied based on the other peer's guarantees on the OverlapChannel),
    /// - an [`OverlappingAoiOutput`], a producer which emits the AoIs submitted both by the other peer and yourself for which there is an overlap, and
    /// - an [`AoiHandles`], which lets you resolve your and your peer's read capability handles to Aois.
    ///
    /// Internally, everything undergoes a bunch of cryptographic verification, and resource-handle-based backpressure.
    ///
    /// In the WGPS, peers can not only *submit* AoIs but also *revoke* them again (via ResourceHandleFree messages). This implementation punts on that functionality. You cannot undeclare any interest, and all requests by the other peer to remove an AoI will be ignored. This makes us selfish peers, and long-term this functionality should be implemented.
    pub fn new(
        state_ref: StateRef,
        overlap_channel_sender: ChannelSender<4, LcmuxStateRef, P, PR, C, CR>,
        overlap_channel_receiver: ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
        announce_overlap_producer: AnnounceOverlapProducer,
        capability_channel_receiver: ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
    ) -> Self {
        Self {
            my_aoi_input: MyAoiInput {
                session_state: state_ref.clone(),
                overlap_channel_sender,
            },
            overlap_output: OverlappingAoiOutput::new(
                state_ref,
                overlap_channel_receiver,
                announce_overlap_producer,
                capability_channel_receiver,
            ),
            // capability_handles: todo!(),
        }
    }
}

/// This is where you submit your own capable AoIs. More specifically, you asynchronously (backpressure based on the OverlapChannel) submit [`CapableAoi`]s. When a submitted AoI overlaps an AoI transmitted by the peer, both are emitted on the [`OverlappingAoiOutput`] corresponding to self.
pub(crate) struct MyAoiInput<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    ReadCap,
    EnumCap,
    P,
    PR,
    C,
    CR,
    StateRef: Deref<
        Target = State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            ReadCap,
            EnumCap,
            P,
            PR,
            C,
            CR,
            LcmuxStateRef,
        >,
    >,
    LcmuxStateRef,
> where
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
{
    session_state: StateRef,
    overlap_channel_sender: ChannelSender<4, LcmuxStateRef, P, PR, C, CR>,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Hash,
        S: SubspaceId + Hash,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef: Deref<
            Target = State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        >,
        LcmuxStateRef,
    > Consumer
    for MyAoiInput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
    >
where
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Eq
        + Hash
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
    EnumCap: Clone
        + Eq
        + Hash
        + EnumerationCapability<Receiver = S, NamespaceId = N>
        + RelativeEncodableKnownSize<(N, S)>,
    P: Producer,
    C: Consumer<Item = u8> + BulkConsumer,
    C::Final: Clone,
    C::Error: Clone,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>>,
{
    type Item = CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>;

    type Final = ();

    type Error = AoiInputError<C::Error>;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        let state = self.session_state.deref();

        let mut interest_registry = state.interest_registry.write().await;

        match interest_registry.submit_capable_aoi(item.clone()) {
            Left(((fst_hash, fst_handle), snd)) => {
                let fst_message = PioBindHash {
                    hash: fst_hash,
                    actually_interested: true,
                };
                self.overlap_channel_sender
                    .send_to_channel(&fst_message)
                    .await?;

                if let Some((snd_hash, _snd_handle)) = snd {
                    let snd_message = PioBindHash {
                        hash: snd_hash,
                        actually_interested: true,
                    };
                    self.overlap_channel_sender
                        .send_to_channel(&snd_message)
                        .await?;
                }

                let overlaps = interest_registry
                    .sent_pio_bind_hash_msgs(fst_handle, &item.to_private_interest());

                for overlap in overlaps {
                    state
                        .process_overlap(&overlap, &mut interest_registry)
                        .await?;
                }

                Ok(())
            }
            Right(overlaps) => {
                let overlaps = overlaps.clone();
                for overlap in overlaps {
                    state
                        .process_overlap(&overlap, &mut interest_registry)
                        .await?;
                }

                Ok(())
            }
        }
    }

    async fn close(&mut self, _fin: Self::Final) -> Result<(), Self::Error> {
        self.overlap_channel_sender
            .close()
            .await
            .map_err(|err| AoiInputError::Transport(err))?;
        Ok(())
    }
}

pub struct OverlappingAoiOutput<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId + Hash + 'static,
    S: SubspaceId + Hash + 'static,
    ReadCap,
    EnumCap,
    P,
    PR,
    C,
    CR,
    StateRef: Deref<
            Target = State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + Borrow<
            State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
        > + 'static,
    LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>> + 'static,
    AnnounceOverlapProducer,
> where
    P: Producer + 'static,
    C: Consumer + 'static,
    PR: Deref<Target = shared_producer::State<P>> + Clone + 'static,
    CR: Deref<Target = shared_consumer::State<C>> + Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    ReadCap: ReadCapability<MCL, MCC, MPL>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + 'static,
    EnumCap: 'static,
{
    session_state: StateRef,
    pio_inputs: Merge<
        MapItem<
            AnnounceOverlapProducer,
            fn(
                PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            ) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap>,
        >,
        Merge<
            MapItem<
                Decoder<
                    ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
                    PioBindHash<INTEREST_HASH_LENGTH>,
                >,
                fn(
                    PioBindHash<INTEREST_HASH_LENGTH>,
                )
                    -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap>,
            >,
            MapItem<
                RelativeDecoder<
                    ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
                    PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
                    StateRef,
                    State<
                        SALT_LENGTH,
                        INTEREST_HASH_LENGTH,
                        MCL,
                        MCC,
                        MPL,
                        N,
                        S,
                        ReadCap,
                        EnumCap,
                        P,
                        PR,
                        C,
                        CR,
                        LcmuxStateRef,
                    >,
                    Blame,
                >,
                fn(
                    PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
                )
                    -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap>,
            >,
        >,
    >,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Hash + 'static,
        S: SubspaceId + Hash + 'static,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef: Deref<
                Target = State<
                    SALT_LENGTH,
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    ReadCap,
                    EnumCap,
                    P,
                    PR,
                    C,
                    CR,
                    LcmuxStateRef,
                >,
            > + Borrow<
                State<
                    SALT_LENGTH,
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    ReadCap,
                    EnumCap,
                    P,
                    PR,
                    C,
                    CR,
                    LcmuxStateRef,
                >,
            > + Clone
            + 'static,
        LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>> + 'static,
        AnnounceOverlapProducer,
    >
    OverlappingAoiOutput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
        AnnounceOverlapProducer,
    >
where
    P: Producer + 'static,
    C: BulkConsumer<Error: Clone, Final: Clone, Item = u8> + 'static,
    PR: Deref<Target = shared_producer::State<P>> + Clone + 'static,
    CR: Deref<Target = shared_consumer::State<C>> + Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
        + 'static,
    EnumCap: Clone
        + Hash
        + Eq
        + 'static
        + RelativeEncodableKnownSize<(N, S)>
        + EnumerationCapability<NamespaceId = N, Receiver = S>,
{
    fn new(
        session_state: StateRef,
        overlap_channel_receiver: ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
        announce_overlap_producer: AnnounceOverlapProducer,
        capability_channel_receiver: ChannelReceiver<4, LcmuxStateRef, P, PR, C, CR>,
    ) -> Self {
        let bind_hash_decoder: Decoder<_, PioBindHash<INTEREST_HASH_LENGTH>> =
            Decoder::new(overlap_channel_receiver);
        let bind_read_capability_decoder: RelativeDecoder<
            _,
            PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
            _,
            State<
                SALT_LENGTH,
                INTEREST_HASH_LENGTH,
                MCL,
                MCC,
                MPL,
                N,
                S,
                ReadCap,
                EnumCap,
                P,
                PR,
                C,
                CR,
                LcmuxStateRef,
            >,
            Blame,
        > = RelativeDecoder::new(capability_channel_receiver, session_state.clone());

        Self {
            session_state,
            pio_inputs: Merge::new(
                MapItem::new(announce_overlap_producer, pio_announce_overlap_to_pio_input),
                Merge::new(
                    MapItem::new(bind_hash_decoder, pio_bind_hash_to_pio_input),
                    MapItem::new(
                        bind_read_capability_decoder,
                        pio_bind_read_capability_to_pio_input,
                    ),
                ),
            ),
        }
    }
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId + Hash + 'static,
        S: SubspaceId + Hash + 'static,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef: Deref<
                Target = State<
                    SALT_LENGTH,
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    ReadCap,
                    EnumCap,
                    P,
                    PR,
                    C,
                    CR,
                    LcmuxStateRef,
                >,
            > + Borrow<
                State<
                    SALT_LENGTH,
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    ReadCap,
                    EnumCap,
                    P,
                    PR,
                    C,
                    CR,
                    LcmuxStateRef,
                >,
            > + 'static,
        LcmuxStateRef: Deref<Target = lcmux::State<4, P, PR, C, CR>> + 'static,
        AnnounceOverlapProducer,
    > Producer
    for OverlappingAoiOutput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        StateRef,
        LcmuxStateRef,
        AnnounceOverlapProducer,
    >
where
    P: Producer + 'static,
    C: BulkConsumer<Error: Clone, Final: Clone, Item = u8> + 'static,
    PR: Deref<Target = shared_producer::State<P>> + Clone + 'static,
    CR: Deref<Target = shared_consumer::State<C>> + Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
        + 'static,
    EnumCap: Clone
        + Hash
        + Eq
        + 'static
        + RelativeEncodableKnownSize<(N, S)>
        + EnumerationCapability<NamespaceId = N, Receiver = S>,
{
    type Item = PioBindReadCapability<MCL, MCC, MPL, ReadCap>;

    type Final = ();

    type Error = AoiOutputError<C::Error>;

    async fn produce(&mut self) -> Result<either::Either<Self::Item, Self::Final>, Self::Error> {
        loop {
            match self.pio_inputs.produce().await? {
                Left(PioInput::BindHash(bind_hash_msg)) => {
                    let mut interest_registry = self.session_state.interest_registry.write().await;

                    let overlaps = interest_registry.received_pio_bind_hash_msg(
                        bind_hash_msg.hash,
                        bind_hash_msg.actually_interested,
                    );

                    for overlap in overlaps {
                        self.session_state
                            .deref()
                            .process_overlap(&overlap, &mut interest_registry)
                            .await?;
                    }

                    // Yay. Go into next iteration of the loop.
                }
                Left(PioInput::AnnounceOverlap(announce_overlap_msg)) => {
                    let mut interest_registry = self.session_state.interest_registry.write().await;

                    let overlap = interest_registry.received_pio_announce_overlap_msg(
                        announce_overlap_msg.sender_handle,
                        announce_overlap_msg.receiver_handle,
                        &announce_overlap_msg.authentication,
                        &announce_overlap_msg.enumeration_capability,
                    )?;

                    self.session_state
                        .deref()
                        .process_overlap(&overlap, &mut interest_registry)
                        .await?;

                    // Yay. Go into next iteration of the loop.
                }
                Left(PioInput::BindReadCapability(bind_read_capability_msg)) => {
                    let mut interest_registry = self.session_state.interest_registry.write().await;

                    let overlap = interest_registry
                        .received_pio_bind_read_capability_msg(&bind_read_capability_msg)?;

                    self.session_state
                        .deref()
                        .process_overlap(&overlap, &mut interest_registry)
                        .await?;

                    // Yay. But instead of going into the next iteration, we can actually emit the received message.
                    return Ok(Left(bind_read_capability_msg));
                }
                Right(()) => return Ok(Right(())),
            }
        }
    }
}

// pub(crate) struct AoiHandles;

/// The information you submit to pio detection (basically the information you need for a `PioBindReadCapability` message).
#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CapableAoi<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap, EnumCap>
{
    /// The read capability for the area one is interested in.
    capability: ReadCap,
    /// The max_count of the AreaOfInterest that the sender wants to sync.
    max_count: u64,
    /// The max_size of the AreaOfInterest that the sender wants to sync.
    max_size: u64,
    max_payload_power: u8,
    /// Must only be `None` if it is impossible to have an awkward pair with the PrivateInterest derived from the `capability`. Otherwise, things may panic.
    enum_cap: Option<EnumCap>,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, ReadCap, EnumCap, N, S>
    CapableAoi<MCL, MCC, MPL, ReadCap, EnumCap>
where
    ReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>,
    N: NamespaceId,
    S: SubspaceId,
{
    fn to_private_interest(&self) -> PrivateInterest<MCL, MCC, MPL, N, S> {
        let area = self.capability.granted_area();
        PrivateInterest::new(
            self.capability.granted_namespace().clone(),
            area.subspace().clone(),
            area.path().clone(),
        )
    }
}

/// The semantics that a valid read capability must provide to be usable with the WGPS.
pub trait ReadCapability<const MCL: usize, const MCC: usize, const MPL: usize> {
    type Receiver;
    type NamespaceId;
    type SubspaceId;

    fn granted_area(&self) -> Area<MCL, MCC, MPL, Self::SubspaceId>;
    fn granted_namespace(&self) -> Self::NamespaceId;
}

/// The semantics that a valid enumeration capability must provide to be usable with the WGPS.
pub trait EnumerationCapability {
    type Receiver;
    type NamespaceId;

    fn granted_namespace(&self) -> Self::NamespaceId;
    fn receiver(&self) -> Self::Receiver;
}

/// Everything that can go wrong when submitting an AreaOfInterest to the private interest overlap detection process.
pub enum AoiInputError<TransportError> {
    Transport(TransportError),
    /// The peer has closed the OverlapChannel, so we cannot send on it any longer.
    /// This error is non-fatal. You must not attempt inputting more Aois, but the remainder of the sync session can continue running without problems.
    OverlapChannelClosed,
    /// The peer has closed the CapabilityChannel, so we cannot send on it any longer.
    /// This error is non-fatal. You must not attempt inputting more Aois, but the remainder of the sync session can continue running without problems.
    CapabilityChannelClosed,
    /// We have to supply an enumeration capability, but the submitted CapableAoi didn't supply one.
    NoEnumerationCapability,
}

impl<TransportError> From<lcmux::LogicalChannelClientError<TransportError>>
    for AoiInputError<TransportError>
{
    fn from(value: lcmux::LogicalChannelClientError<TransportError>) -> Self {
        match value {
            lcmux::LogicalChannelClientError::LogicalChannelClosed => {
                AoiInputError::OverlapChannelClosed
            }
            lcmux::LogicalChannelClientError::Underlying(err) => AoiInputError::Transport(err),
        }
    }
}

enum ProcessOverlapError<TransportError> {
    Transport(TransportError),
    /// The peer has closed the CapabilityChannel, so we cannot send on it any longer.
    /// This error is non-fatal. You must not attempt inputting more Aois, but the remainder of the sync session can continue running without problems.
    CapabilityChannelClosed,
    /// We have to supply an enumeration capability, but the submitted CapableAoi didn't supply one.
    NoEnumerationCapability,
}

impl<TransportError> From<lcmux::LogicalChannelClientError<TransportError>>
    for ProcessOverlapError<TransportError>
{
    fn from(value: lcmux::LogicalChannelClientError<TransportError>) -> Self {
        match value {
            lcmux::LogicalChannelClientError::LogicalChannelClosed => {
                ProcessOverlapError::CapabilityChannelClosed
            }
            lcmux::LogicalChannelClientError::Underlying(err) => {
                ProcessOverlapError::Transport(err)
            }
        }
    }
}

impl<TransportError> From<ProcessOverlapError<TransportError>> for AoiInputError<TransportError> {
    fn from(value: ProcessOverlapError<TransportError>) -> Self {
        match value {
            ProcessOverlapError::Transport(err) => AoiInputError::Transport(err),
            ProcessOverlapError::CapabilityChannelClosed => AoiInputError::CapabilityChannelClosed,
            ProcessOverlapError::NoEnumerationCapability => AoiInputError::NoEnumerationCapability,
        }
    }
}

/// Everything that can go wrong when receiving AreaOfInterests from the peer.
pub enum AoiOutputError<TransportError> {
    Transport(TransportError),
    // The peer sent some data that didn't decode properly (this includes sending invalid capabilities).
    InvalidData,
    /// The peer has closed the CapabilityChannel, so we cannot send on it any longer.
    /// This error is non-fatal. You must not attempt inputting more Aois, but the remainder of the sync session can continue running without problems.
    CapabilityChannelClosed,
    /// We have to supply an enumeration capability, but the submitted CapableAoi didn't supply one.
    NoEnumerationCapability,
    ReceivedAnnouncementError(ReceivedAnnouncementError),
    ReceivedBindCapabilityError(ReceivedBindCapabilityError),
}

impl<TransportError> From<DecodeError<(), (), Blame>> for AoiOutputError<TransportError> {
    fn from(value: DecodeError<(), (), Blame>) -> Self {
        Self::InvalidData
    }
}

impl<TransportError> From<ProcessOverlapError<TransportError>> for AoiOutputError<TransportError> {
    fn from(value: ProcessOverlapError<TransportError>) -> Self {
        match value {
            ProcessOverlapError::Transport(err) => Self::Transport(err),
            ProcessOverlapError::CapabilityChannelClosed => Self::CapabilityChannelClosed,
            ProcessOverlapError::NoEnumerationCapability => Self::NoEnumerationCapability,
        }
    }
}

impl<TransportError> From<ReceivedAnnouncementError> for AoiOutputError<TransportError> {
    fn from(value: ReceivedAnnouncementError) -> Self {
        Self::ReceivedAnnouncementError(value)
    }
}

impl<TransportError> From<ReceivedBindCapabilityError> for AoiOutputError<TransportError> {
    fn from(value: ReceivedBindCapabilityError) -> Self {
        Self::ReceivedBindCapabilityError(value)
    }
}

/// The different kinds of data (messages) the other peer can send to us that impact pio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PioInput<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    ReadCap,
    EnumCap,
> {
    BindHash(PioBindHash<INTEREST_HASH_LENGTH>),
    AnnounceOverlap(PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>),
    BindReadCapability(PioBindReadCapability<MCL, MCC, MPL, ReadCap>),
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        ReadCap,
        EnumCap,
        P,
        PR,
        C,
        CR,
        LcmuxStateRef,
        RC,
    >
    RelativeDecodable<
        State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            ReadCap,
            EnumCap,
            P,
            PR,
            C,
            CR,
            LcmuxStateRef,
        >,
        Blame,
    > for PioBindReadCapability<MCL, MCC, MPL, RC>
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer,
    C: Consumer,
    PR: Deref<Target = shared_producer::State<P>> + Clone,
    CR: Deref<Target = shared_consumer::State<C>> + Clone,
    RC: ReadCapability<MCL, MCC, MPL>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>,
{
    async fn relative_decode<Pro>(
        producer: &mut Pro,
        r: &State<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            ReadCap,
            EnumCap,
            P,
            PR,
            C,
            CR,
            LcmuxStateRef,
        >,
    ) -> Result<Self, DecodeError<Pro::Final, Pro::Error, Blame>>
    where
        Pro: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let sender_handle_tag = compact_u64::Tag::from_raw(header, compact_u64::TagWidth::two(), 0);
        let receiver_handle_tag =
            compact_u64::Tag::from_raw(header, compact_u64::TagWidth::two(), 2);
        let max_count_tag = compact_u64::Tag::from_raw(header, compact_u64::TagWidth::two(), 4);
        let max_size_tag = compact_u64::Tag::from_raw(header, compact_u64::TagWidth::two(), 6);

        let sender_handle = CompactU64::relative_decode(producer, &sender_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let receiver_handle = CompactU64::relative_decode(producer, &receiver_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let max_count = CompactU64::relative_decode(producer, &max_count_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let max_size = CompactU64::relative_decode(producer, &max_size_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let max_payload_power = producer.produce_item().await?;

        let private_interest = r
            .interest_registry
            .read()
            .await
            .hash_registry
            .try_get_our_handle_info(receiver_handle)
            .ok_or(DecodeError::Other(Blame::TheirFault))?
            .0
            .private_interest
            .clone();

        let ppi = PersonalPrivateInterest {
            user_key: r.their_public_key.clone(),
            private_interest: private_interest,
        };

        let capability = RC::relative_decode(producer, &ppi).await?;

        Ok(Self {
            sender_handle,
            receiver_handle,
            max_count,
            max_size,
            max_payload_power,
            capability,
        })
    }
}

fn pio_announce_overlap_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    ReadCap,
    EnumCap,
>(
    msg: PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap> {
    PioInput::AnnounceOverlap(msg)
}

fn pio_bind_hash_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    ReadCap,
    EnumCap,
>(
    msg: PioBindHash<INTEREST_HASH_LENGTH>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap> {
    PioInput::BindHash(msg)
}

fn pio_bind_read_capability_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    ReadCap,
    EnumCap,
>(
    msg: PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, ReadCap, EnumCap> {
    PioInput::BindReadCapability(msg)
}
