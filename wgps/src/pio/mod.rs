use std::{borrow::Borrow, cell::RefCell, collections::HashSet, hash::Hash, ops::Deref, rc::Rc};

use compact_u64::CompactU64;
use either::Either::{Left, Right};
use lcmux::{ChannelReceiver, ChannelSender, GlobalMessageSender};
use ufotofu::{
    producer::{MapItem, Merge},
    BulkConsumer, Consumer, Producer,
};
use ufotofu_codec::{
    adaptors::{Decoder, RelativeDecoder},
    Blame, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
    RelativeEncodableKnownSize,
};
use ufotofu_queues::{Fixed, Static};
use wb_async_utils::{
    rw::WriteGuard,
    shared_consumer::{self, SharedConsumer},
    shared_producer::{self, SharedProducer},
    spsc, Mutex, RwLock,
};
use willow_data_model::{
    grouping::{Area, AreaOfInterest, AreaSubspace},
    NamespaceId, Path, SubspaceId,
};
use willow_pio::{PersonalPrivateInterest, PrivateInterest};

use crate::{
    messages::{PioAnnounceOverlap, PioBindHash, PioBindReadCapability},
    pio::{
        capable_aois::{InterestRegistry, ReceivedAnnouncementError, ReceivedBindCapabilityError},
        overlap_finder::{NamespacedAoIWithMaxPayloadPower, OverlapFinder},
        salted_hashes::Overlap,
    },
};

mod capable_aois;
mod overlap_finder;
mod salted_hashes;

/// The state for a PIO session.
#[derive(Debug)]
struct State<
    const SALT_LENGTH: usize,
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    MyReadCap,
    MyEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
> {
    p: SharedProducer<Rc<shared_producer::State<P, PFinal, PErr>>, P, PFinal, PErr>,
    c: SharedConsumer<Rc<shared_consumer::State<C, CErr>>, C, CErr>,
    interest_registry: RwLock<
        InterestRegistry<
            SALT_LENGTH,
            INTEREST_HASH_LENGTH,
            MCL,
            MCC,
            MPL,
            N,
            S,
            MyReadCap,
            MyEnumCap,
        >,
    >,
    caois_for_which_we_already_sent_a_bind_read_capability_message:
        RefCell<HashSet<CapableAoi<MCL, MCC, MPL, MyReadCap, MyEnumCap>>>,
    global_sender: Mutex<GlobalMessageSender<4, P, PFinal, PErr, C, CErr>>,
    capability_channel_sender: Mutex<ChannelSender<4, P, PFinal, PErr, C, CErr>>,
    my_public_key: S,
    their_public_key: S,
    // After we have sent a PioBindReadCapability message, we also put its data in here, for later overlap reporting.
    read_capabilities_i_sent_sender: Mutex<
        spsc::Sender<
            Rc<
                spsc::State<
                    Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
                    (),
                    DecodeError<(), (), Blame>,
                >,
            >,
            Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
            (),
            DecodeError<(), (), Blame>,
        >,
    >,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    >
    State<
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
    >
where
    N: Default + Clone,
    S: Clone,
{
    /// Create a new opaque state for a PIO session.
    fn new(
        p: SharedProducer<Rc<shared_producer::State<P, PFinal, PErr>>, P, PFinal, PErr>,
        c: SharedConsumer<Rc<shared_consumer::State<C, CErr>>, C, CErr>,
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
        my_public_key: S,
        their_public_key: S,
        global_sender: Mutex<GlobalMessageSender<4, P, PFinal, PErr, C, CErr>>,
        capability_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
        read_capabilities_i_sent_sender: spsc::Sender<
            Rc<
                spsc::State<
                    Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
                    (),
                    DecodeError<(), (), Blame>,
                >,
            >,
            Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
            (),
            DecodeError<(), (), Blame>,
        >,
    ) -> Self {
        Self {
            p,
            c,
            interest_registry: RwLock::new(InterestRegistry::new(my_salt, h)),
            caois_for_which_we_already_sent_a_bind_read_capability_message: RefCell::new(
                HashSet::new(),
            ),
            global_sender,
            capability_channel_sender: Mutex::new(capability_channel_sender),
            my_public_key,
            their_public_key,
            read_capabilities_i_sent_sender: Mutex::new(read_capabilities_i_sent_sender),
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
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    >
    State<
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
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    MyReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Eq
        + Hash
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
    MyEnumCap: Eq
        + Hash
        + Clone
        + EnumerationCapability<Receiver = S, NamespaceId = N>
        + RelativeEncodableKnownSize<(N, S)>,
    C: Consumer<Item = u8, Final = (), Error = CErr> + BulkConsumer,
    CErr: Clone,
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
                MyReadCap,
                MyEnumCap,
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

                    self.read_capabilities_i_sent_sender
                        .write()
                        .await
                        .consume(NamespacedAoIWithMaxPayloadPower::from_bind_read_capability_msg(&msg))
                        .await.unwrap(/* infallible */);

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

                let msg: PioAnnounceOverlap<INTEREST_HASH_LENGTH, MyEnumCap> = PioAnnounceOverlap {
                    sender_handle: overlap.my_interesting_handle,
                    receiver_handle: overlap.their_handle,
                    authentication,
                    enumeration_capability,
                };

                let pair = (
                    caoi.capability.granted_namespace().clone(),
                    self.my_public_key.clone(),
                );

                self.global_sender
                    .write()
                    .await
                    .send_global_message_relative(msg, &pair)
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
    N,
    S,
    MyReadCap,
    MyEnumCap,
    TheirReadCap,
    TheirEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
    AnnounceOverlapProducer,
> {
    pub my_aoi_input: MyAoiInput<
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
    pub overlap_output: OverlappingAoiOutput<
        SALT_LENGTH,
        INTEREST_HASH_LENGTH,
        MCL,
        MCC,
        MPL,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
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
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
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
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        AnnounceOverlapProducer,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer<Final = PFinal, Error = PErr> + 'static,
    PFinal: 'static,
    PErr: 'static,
    C: BulkConsumer<Item = u8, Final: Clone, Error = CErr> + 'static,
    CErr: Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, TheirEnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    MyReadCap: 'static,
    MyEnumCap: 'static,
    TheirReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
        + 'static,
    TheirEnumCap: Clone
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
        p: SharedProducer<Rc<shared_producer::State<P, PFinal, PErr>>, P, PFinal, PErr>,
        c: SharedConsumer<Rc<shared_consumer::State<C, CErr>>, C, CErr>,
        my_salt: [u8; SALT_LENGTH],
        h: fn(
            &PrivateInterest<MCL, MCC, MPL, N, S>,
            &[u8; SALT_LENGTH],
        ) -> [u8; INTEREST_HASH_LENGTH],
        my_public_key: S,
        their_public_key: S,
        global_sender: Mutex<GlobalMessageSender<4, P, PFinal, PErr, C, CErr>>,
        capability_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
        capability_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
        overlap_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
        overlap_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
        announce_overlap_producer: AnnounceOverlapProducer,
    ) -> Self {
        let my_sent_caois_state = Rc::new(spsc::State::new(Static::new()));
        let (my_sent_caois_sender, my_sent_caois_receiver) = spsc::new_spsc(my_sent_caois_state);

        let state_ref = Rc::new(State::new(
            p,
            c,
            my_salt,
            h,
            my_public_key,
            their_public_key,
            global_sender,
            capability_channel_sender,
            my_sent_caois_sender,
        ));

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
                my_sent_caois_receiver,
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
    N,
    S,
    MyReadCap,
    MyEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
> {
    session_state: Rc<
        State<
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
    overlap_channel_sender: ChannelSender<4, P, PFinal, PErr, C, CErr>,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    > Consumer
    for MyAoiInput<
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
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    MyReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Eq
        + Hash
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
    MyEnumCap: Eq
        + Hash
        + Clone
        + EnumerationCapability<Receiver = S, NamespaceId = N>
        + RelativeEncodableKnownSize<(N, S)>,
    C: Consumer<Item = u8, Final = (), Error = CErr> + BulkConsumer,
    CErr: Clone,
{
    type Item = CapableAoi<MCL, MCC, MPL, MyReadCap, MyEnumCap>;

    type Final = ();

    type Error = AoiInputError<CErr>;

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
    N,
    S,
    MyReadCap,
    MyEnumCap,
    TheirReadCap,
    TheirEnumCap,
    P,
    PFinal,
    PErr,
    C,
    CErr,
    AnnounceOverlapProducer,
> {
    session_state: Rc<
        State<
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
    overlap_finder: OverlapFinder<MCL, MCC, MPL, N, S>,
    pio_inputs: Merge<
        Merge<
            MapItem<
                spsc::Receiver<
                    Rc<
                        spsc::State<
                            Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
                            (),
                            DecodeError<(), (), Blame>,
                        >,
                    >,
                    Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
                    (),
                    DecodeError<(), (), Blame>,
                >,
                fn(
                    NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
                ) -> PioInput<
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    TheirReadCap,
                    TheirEnumCap,
                >,
            >,
            MapItem<
                AnnounceOverlapProducer,
                fn(
                    PioAnnounceOverlap<INTEREST_HASH_LENGTH, TheirEnumCap>,
                ) -> PioInput<
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    TheirReadCap,
                    TheirEnumCap,
                >,
            >,
            PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, TheirReadCap, TheirEnumCap>,
            (),
            DecodeError<(), (), Blame>,
        >,
        Merge<
            MapItem<
                Decoder<
                    ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
                    PioBindHash<INTEREST_HASH_LENGTH>,
                >,
                fn(
                    PioBindHash<INTEREST_HASH_LENGTH>,
                ) -> PioInput<
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    TheirReadCap,
                    TheirEnumCap,
                >,
            >,
            MapItem<
                RelativeDecoder<
                    ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
                    PioBindReadCapability<MCL, MCC, MPL, TheirReadCap>,
                    Rc<
                        State<
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
                    State<
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
                    Blame,
                >,
                fn(
                    PioBindReadCapability<MCL, MCC, MPL, TheirReadCap>,
                ) -> PioInput<
                    INTEREST_HASH_LENGTH,
                    MCL,
                    MCC,
                    MPL,
                    N,
                    S,
                    TheirReadCap,
                    TheirEnumCap,
                >,
            >,
            PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, TheirReadCap, TheirEnumCap>,
            (),
            DecodeError<(), (), Blame>,
        >,
        PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, TheirReadCap, TheirEnumCap>,
        (),
        DecodeError<(), (), Blame>,
    >,
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
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
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        AnnounceOverlapProducer,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer<Final = PFinal, Error = PErr> + 'static,
    PFinal: 'static,
    PErr: 'static,
    C: BulkConsumer<Item = u8, Final: Clone, Error = CErr> + 'static,
    CErr: Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, TheirEnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    MyReadCap: 'static,
    MyEnumCap: 'static,
    TheirReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + 'static,
    TheirEnumCap:
        Clone + Hash + Eq + 'static + EnumerationCapability<NamespaceId = N, Receiver = S>,
{
    fn new(
        session_state: Rc<
            State<
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
        overlap_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
        announce_overlap_producer: AnnounceOverlapProducer,
        capability_channel_receiver: ChannelReceiver<4, P, PFinal, PErr, C, CErr>,
        my_sent_read_capabilities_receiver: spsc::Receiver<
            Rc<
                spsc::State<
                    Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
                    (),
                    DecodeError<(), (), Blame>,
                >,
            >,
            Static<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>, 32>,
            (),
            DecodeError<(), (), Blame>,
        >,
    ) -> Self {
        let bind_hash_decoder: Decoder<_, PioBindHash<INTEREST_HASH_LENGTH>> =
            Decoder::new(overlap_channel_receiver);
        let bind_read_capability_decoder: RelativeDecoder<
            _,
            PioBindReadCapability<MCL, MCC, MPL, TheirReadCap>,
            _,
            State<
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
            Blame,
        > = RelativeDecoder::new(capability_channel_receiver, session_state.clone());

        Self {
            session_state,
            overlap_finder: OverlapFinder::new(),
            pio_inputs: Merge::new(
                Merge::new(
                    MapItem::new(
                        my_sent_read_capabilities_receiver,
                        namespaced_aoi_with_max_payload_power_to_pio_input,
                    ),
                    MapItem::new(announce_overlap_producer, pio_announce_overlap_to_pio_input),
                ),
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
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
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
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
        AnnounceOverlapProducer,
    >
where
    N: NamespaceId + Hash,
    S: SubspaceId + Hash,
    P: Producer<Final = PFinal, Error = PErr> + 'static,
    PFinal: 'static,
    PErr: 'static,
    C: BulkConsumer<Item = u8, Final = (), Error = CErr> + 'static,
    CErr: Clone + 'static,
    AnnounceOverlapProducer: Producer<
            Item = PioAnnounceOverlap<INTEREST_HASH_LENGTH, TheirEnumCap>,
            Final = (),
            Error = DecodeError<(), (), Blame>,
        > + 'static,
    MyReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + Hash
        + Eq
        + Clone
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
        + 'static,
    MyEnumCap: Clone
        + Hash
        + Eq
        + RelativeEncodableKnownSize<(N, S)>
        + EnumerationCapability<NamespaceId = N, Receiver = S>
        + 'static,
    TheirReadCap: ReadCapability<MCL, MCC, MPL, NamespaceId = N, SubspaceId = S>
        + RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
        + Hash
        + Eq
        + Clone
        + 'static,
    TheirEnumCap:
        Clone + Hash + Eq + 'static + EnumerationCapability<NamespaceId = N, Receiver = S>,
{
    type Item = Vec<NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>>;

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
                Left(PioInput::WeReceivedAReadCapability(bind_read_capability_msg)) => {
                    let mut interest_registry = self.session_state.interest_registry.write().await;

                    let overlap = interest_registry
                        .received_pio_bind_read_capability_msg(&bind_read_capability_msg)?;

                    self.session_state
                        .deref()
                        .process_overlap(&overlap, &mut interest_registry)
                        .await?;

                    // Yay. Record the caoi, and check for any detected overlaps.
                    let proper_overlaps = self.overlap_finder.add_theirs(
                        NamespacedAoIWithMaxPayloadPower::from_bind_read_capability_msg(
                            &bind_read_capability_msg,
                        ),
                    );

                    if proper_overlaps.len() == 0 {
                        // Do nothing, go to next iteration
                    } else {
                        return Ok(Left(proper_overlaps));
                    }
                }

                Left(PioInput::WeSentAReadCapability(naoiwmpp)) => {
                    // Yay. Record the caoi, and check for any detected overlaps.
                    let proper_overlaps = self.overlap_finder.add_mine(naoiwmpp);

                    if proper_overlaps.len() == 0 {
                        // Do nothing, go to next iteration
                    } else {
                        return Ok(Left(proper_overlaps));
                    }
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
    type NamespaceId;
    type SubspaceId;

    fn granted_area(&self) -> &Area<MCL, MCC, MPL, Self::SubspaceId>;
    fn granted_namespace(&self) -> &Self::NamespaceId;
}

/// The semantics that a valid enumeration capability must provide to be usable with the WGPS.
pub trait EnumerationCapability {
    type Receiver;
    type NamespaceId;

    fn granted_namespace(&self) -> &Self::NamespaceId;
    fn receiver(&self) -> &Self::Receiver;
}

/// The simplified data of a read capability, stripping all verification-related information.
struct SimplifiedReadCapability<const MCL: usize, const MCC: usize, const MPL: usize, N, S> {
    area: Area<MCL, MCC, MPL, S>,
    namespace: N,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S> ReadCapability<MCL, MCC, MPL>
    for SimplifiedReadCapability<MCL, MCC, MPL, N, S>
{
    type NamespaceId = N;
    type SubspaceId = S;

    fn granted_namespace(&self) -> &Self::NamespaceId {
        &self.namespace
    }

    fn granted_area(&self) -> &Area<MCL, MCC, MPL, Self::SubspaceId> {
        &self.area
    }
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

impl<TransportError> From<TransportError> for ProcessOverlapError<TransportError> {
    fn from(value: TransportError) -> Self {
        ProcessOverlapError::Transport(value)
    }
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
    fn from(_value: DecodeError<(), (), Blame>) -> Self {
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
    N,
    S,
    TheirReadCap,
    TheirEnumCap,
> {
    BindHash(PioBindHash<INTEREST_HASH_LENGTH>),
    AnnounceOverlap(PioAnnounceOverlap<INTEREST_HASH_LENGTH, TheirEnumCap>),
    WeReceivedAReadCapability(PioBindReadCapability<MCL, MCC, MPL, TheirReadCap>),
    WeSentAReadCapability(NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>),
}

fn pio_announce_overlap_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    EnumCap,
>(
    msg: PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap> {
    PioInput::AnnounceOverlap(msg)
}

fn pio_bind_hash_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    EnumCap,
>(
    msg: PioBindHash<INTEREST_HASH_LENGTH>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap> {
    PioInput::BindHash(msg)
}

fn pio_bind_read_capability_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    EnumCap,
>(
    msg: PioBindReadCapability<MCL, MCC, MPL, ReadCap>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap> {
    PioInput::WeReceivedAReadCapability(msg)
}

fn namespaced_aoi_with_max_payload_power_to_pio_input<
    const INTEREST_HASH_LENGTH: usize,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    ReadCap,
    EnumCap,
>(
    naoiwmpp: NamespacedAoIWithMaxPayloadPower<MCL, MCC, MPL, N, S>,
) -> PioInput<INTEREST_HASH_LENGTH, MCL, MCC, MPL, N, S, ReadCap, EnumCap> {
    PioInput::WeSentAReadCapability(naoiwmpp)
}

impl<
        const SALT_LENGTH: usize,
        const INTEREST_HASH_LENGTH: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirReadCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
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
            MyReadCap,
            MyEnumCap,
            P,
            PFinal,
            PErr,
            C,
            CErr,
        >,
        Blame,
    > for PioBindReadCapability<MCL, MCC, MPL, TheirReadCap>
where
    N: Clone,
    S: Clone,
    TheirReadCap: RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>,
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
            MyReadCap,
            MyEnumCap,
            P,
            PFinal,
            PErr,
            C,
            CErr,
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

        let capability = TheirReadCap::relative_decode(producer, &ppi).await?;

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
