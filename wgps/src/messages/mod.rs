use std::rc::Rc;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::{CompactU64, TagWidth};
use either::Either::{Left, Right};
use willow_data_model::{grouping::Range3d, LengthyAuthorisedEntry, NamespaceId, SubspaceId};

use crate::{
    parameters::{EnumerationCapability, ReadCapability},
    pio,
};
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable, RelativeEncodableKnownSize,
};
use willow_encoding::is_bitflagged;
use willow_pio::PersonalPrivateInterest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GlobalMessage<const INTEREST_HASH_LENGTH: usize, EnumCap> {
    ResourceHandleFree(ResourceHandleFree),
    DataSetEagerness(DataSetEagerness),
    PioAnnounceOverlap(PioAnnounceOverlap<INTEREST_HASH_LENGTH, EnumCap>),
}

impl<
        const HANDSHAKE_HASHLEN_IN_BYTES: usize, // This is also the PIO SALT_LENGTH
        const PIO_INTEREST_HASH_LENGTH_IN_BYTES: usize,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        S,
        MyReadCap,
        MyEnumCap,
        TheirEnumCap,
        P,
        PFinal,
        PErr,
        C,
        CErr,
    >
    RelativeDecodable<
        Option<
            Rc<
                pio::State<
                    HANDSHAKE_HASHLEN_IN_BYTES,
                    PIO_INTEREST_HASH_LENGTH_IN_BYTES,
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
        >,
        Blame,
    > for GlobalMessage<PIO_INTEREST_HASH_LENGTH_IN_BYTES, TheirEnumCap>
where
    N: Clone,
    S: Clone,
    TheirEnumCap: RelativeDecodable<(N, S), Blame>,
{
    async fn relative_decode<Pro>(
        producer: &mut Pro,
        r: &Option<
            Rc<
                pio::State<
                    HANDSHAKE_HASHLEN_IN_BYTES,
                    PIO_INTEREST_HASH_LENGTH_IN_BYTES,
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
        >,
    ) -> Result<Self, DecodeError<Pro::Final, Pro::Error, Blame>>
    where
        Pro: ufotofu::BulkProducer<Item = u8>,
    {
        match producer.expose_items().await? {
            Left(bytes) => {
                let first_byte = bytes[0];

                if first_byte & 0b1100_0000 == 0b1000_0000 {
                    todo!("DataSetEagerness")
                } else if first_byte & 0b1100_0000 == 0b1100_0000 {
                    todo!("ResourceHandleFree")
                } else {
                    PioAnnounceOverlap::relative_decode(producer, r)
                        .await
                        .map(GlobalMessage::PioAnnounceOverlap)
                }
            }
            Right(fin) => return Err(DecodeError::UnexpectedEndOfInput(fin)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceHandleFree {}

impl Encodable for ResourceHandleFree {
    async fn encode<C>(&self, _consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl Decodable for ResourceHandleFree {
    type ErrorReason = Blame;

    async fn decode<P>(
        _producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataSetEagerness {}

impl Encodable for DataSetEagerness {
    async fn encode<C>(&self, _consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl Decodable for DataSetEagerness {
    type ErrorReason = Blame;

    async fn decode<P>(
        _producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

/// Bind data to an OverlapHandle for performing private interest overlap detection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct PioBindHash<const INTEREST_HASH_LENGTH: usize> {
    /// The result of applying hash_interests to a PrivateInterest.
    pub hash: [u8; INTEREST_HASH_LENGTH],

    /// Whether the peer is directly interested in the hashed PrivateInterest, or whether it is merely a relaxation.
    pub actually_interested: bool,
}

impl<const INTEREST_HASH_LENGTH: usize> Encodable for PioBindHash<INTEREST_HASH_LENGTH> {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let header = if self.actually_interested {
            0b1000_0000
        } else {
            0b0000_0000
        };

        consumer.consume(header).await?;

        consumer
            .bulk_consume_full_slice(&self.hash)
            .await
            .map_err(|err| err.into_reason())?;

        Ok(())
    }
}

impl<const INTEREST_HASH_LENGTH: usize> EncodableKnownSize for PioBindHash<INTEREST_HASH_LENGTH> {
    fn len_of_encoding(&self) -> usize {
        1 + INTEREST_HASH_LENGTH
    }
}

impl<const INTEREST_HASH_LENGTH: usize> EncodableSync for PioBindHash<INTEREST_HASH_LENGTH> {}

impl<const INTEREST_HASH_LENGTH: usize> Decodable for PioBindHash<INTEREST_HASH_LENGTH> {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let actually_interested = is_bitflagged(header, 0);

        let mut hash = [0; INTEREST_HASH_LENGTH];

        producer.bulk_overwrite_full_slice(&mut hash).await?;

        Ok(Self {
            actually_interested,
            hash,
        })
    }
}

/// Send an overlap announcement, including its announcement authentication and an optional enumeration capability.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct PioAnnounceOverlap<const INTEREST_HASH_LENGTH: usize, EnumerationCapability> {
    /// The OverlapHandle (bound by the sender of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    pub sender_handle: u64,

    /// The OverlapHandle (bound by the receiver of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    pub receiver_handle: u64,

    /// The announcement authentication for this overlap announcement.
    pub authentication: [u8; INTEREST_HASH_LENGTH],

    /// The enumeration capability if this overlap announcement is for an awkward pair, or none otherwise.
    pub enumeration_capability: Option<EnumerationCapability>,
}

impl<const INTEREST_HASH_LENGTH: usize, EC> PioAnnounceOverlap<INTEREST_HASH_LENGTH, EC> {
    pub(crate) fn temporary() -> Self {
        Self {
            sender_handle: u64::MAX,
            receiver_handle: u64::MAX,
            authentication: [0; INTEREST_HASH_LENGTH],
            enumeration_capability: None,
        }
    }
}

impl<const INTEREST_HASH_LENGTH: usize, N, R, EC> RelativeEncodable<(N, R)>
    for PioAnnounceOverlap<INTEREST_HASH_LENGTH, EC>
where
    N: PartialEq + Clone,
    R: PartialEq + Clone,
    EC: EnumerationCapability<Receiver = R, NamespaceId = N> + RelativeEncodable<(N, R)>,
{
    async fn relative_encode<C>(&self, consumer: &mut C, r: &(N, R)) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0x0;

        if self.enumeration_capability.is_some() {
            header |= 0b0100_0000;
        }

        let sender_handle_tag =
            compact_u64::Tag::min_tag(self.sender_handle, compact_u64::TagWidth::three());

        header |= sender_handle_tag.data_at_offset(2);

        let receiver_handle_tag =
            compact_u64::Tag::min_tag(self.receiver_handle, compact_u64::TagWidth::three());

        header |= receiver_handle_tag.data_at_offset(5);

        consumer.consume(header).await?;

        CompactU64(self.sender_handle)
            .relative_encode(consumer, &sender_handle_tag.encoding_width())
            .await?;

        CompactU64(self.receiver_handle)
            .relative_encode(consumer, &receiver_handle_tag.encoding_width())
            .await?;

        consumer
            .bulk_consume_full_slice(&self.authentication)
            .await
            .map_err(|err| err.into_reason())?;

        if let Some(enumeration_capability) = &self.enumeration_capability {
            let pair = (
                enumeration_capability.granted_namespace().clone(),
                enumeration_capability.receiver().clone(),
            );

            debug_assert!(r.0 == pair.0);
            debug_assert!(r.1 == pair.1);

            enumeration_capability
                .relative_encode(consumer, &pair)
                .await?;
        }

        Ok(())
    }
}

impl<const INTEREST_HASH_LENGTH: usize, N, R, EC> RelativeEncodableKnownSize<(N, R)>
    for PioAnnounceOverlap<INTEREST_HASH_LENGTH, EC>
where
    N: PartialEq + Clone,
    R: PartialEq + Clone,
    EC: EnumerationCapability<Receiver = R, NamespaceId = N> + RelativeEncodableKnownSize<(N, R)>,
{
    fn relative_len_of_encoding(&self, r: &(N, R)) -> usize {
        let sender_handle_tag =
            compact_u64::Tag::min_tag(self.sender_handle, compact_u64::TagWidth::three());

        let receiver_handle_tag =
            compact_u64::Tag::min_tag(self.receiver_handle, compact_u64::TagWidth::three());

        let sender_handle_len = CompactU64(self.sender_handle)
            .relative_len_of_encoding(&sender_handle_tag.encoding_width());

        let receiver_handle_len = CompactU64(self.receiver_handle)
            .relative_len_of_encoding(&receiver_handle_tag.encoding_width());

        let auth_len = self.authentication.len();

        let enum_cap_len = if let Some(enumeration_capability) = &self.enumeration_capability {
            enumeration_capability.relative_len_of_encoding(r)
        } else {
            0
        };

        1 + sender_handle_len + receiver_handle_len + auth_len + enum_cap_len
    }
}

impl<const INTEREST_HASH_LENGTH: usize, N, R, EC> RelativeDecodable<(N, R), Blame>
    for PioAnnounceOverlap<INTEREST_HASH_LENGTH, EC>
where
    EC: RelativeDecodable<(N, R), Blame>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(N, R),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let has_enumeration_cap = is_bitflagged(header, 1);

        let sender_handle_tag =
            compact_u64::Tag::from_raw(header, compact_u64::TagWidth::three(), 2);
        let receiver_handle_tag =
            compact_u64::Tag::from_raw(header, compact_u64::TagWidth::three(), 5);

        let sender_handle = CompactU64::relative_decode(producer, &sender_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let receiver_handle = CompactU64::relative_decode(producer, &receiver_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let mut authentication = [0x0; INTEREST_HASH_LENGTH];

        producer
            .bulk_overwrite_full_slice(&mut authentication)
            .await?;

        let enumeration_capability = if has_enumeration_cap {
            let cap = EC::relative_decode(producer, r)
                .await
                .map_err(DecodeError::map_other_from)?;

            Some(cap)
        } else {
            None
        };

        Ok(Self {
            authentication,
            sender_handle,
            receiver_handle,
            enumeration_capability,
        })
    }
}

/// Bind a read capability for an overlap between two PrivateInterests. Additionally, this message specifies an AreaOfInterest which the sender wants to sync.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct PioBindReadCapability<const MCL: usize, const MCC: usize, const MPL: usize, RC> {
    /// The OverlapHandle (bound by the sender of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    pub sender_handle: u64,

    /// The OverlapHandle (bound by the receiver of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    pub receiver_handle: u64,

    /// The ReadCapability to bind. Its granted namespace must be the (shared) namespace_id of the two PrivateInterests. Its granted area must be included in the less specific of the two PrivateInterests.
    pub capability: RC,

    /// The max_count of the AreaOfInterest that the sender wants to sync.
    pub max_count: u64,

    /// The max_size of the AreaOfInterest that the sender wants to sync.
    pub max_size: u64,

    /// When the receiver of this message eagerly transmits Entries for the AreaOfInterest defined by this message, it must not include the Payload of Entries whose payload_length is strictly greater than two to the power of the max_payload_power. We call the resulting number the sender’s maximum payload size for this AreaOfInterest.
    pub max_payload_power: u8,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, RC>
    RelativeEncodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
    for PioBindReadCapability<MCL, MCC, MPL, RC>
where
    N: NamespaceId,
    S: SubspaceId,
    RC: ReadCapability<MCL, MCC, MPL>
        + RelativeEncodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, S>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0x0;

        let sender_handle_tag =
            compact_u64::Tag::min_tag(self.sender_handle, compact_u64::TagWidth::two());
        header |= sender_handle_tag.data_at_offset(0);

        let receiver_handle_tag =
            compact_u64::Tag::min_tag(self.receiver_handle, compact_u64::TagWidth::two());
        header |= receiver_handle_tag.data_at_offset(2);

        let max_count_tag = compact_u64::Tag::min_tag(self.max_count, compact_u64::TagWidth::two());
        header |= max_count_tag.data_at_offset(4);

        let max_size_tag = compact_u64::Tag::min_tag(self.max_size, compact_u64::TagWidth::two());
        header |= max_size_tag.data_at_offset(6);

        consumer.consume(header).await?;

        CompactU64(self.sender_handle)
            .relative_encode(consumer, &sender_handle_tag.encoding_width())
            .await?;

        CompactU64(self.receiver_handle)
            .relative_encode(consumer, &receiver_handle_tag.encoding_width())
            .await?;

        CompactU64(self.max_count)
            .relative_encode(consumer, &max_count_tag.encoding_width())
            .await?;

        CompactU64(self.max_size)
            .relative_encode(consumer, &max_size_tag.encoding_width())
            .await?;

        consumer.consume(self.max_payload_power).await?;

        self.capability.relative_encode(consumer, r).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, RC>
    RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>
    for PioBindReadCapability<MCL, MCC, MPL, RC>
where
    N: NamespaceId,
    S: SubspaceId,
    RC: ReadCapability<MCL, MCC, MPL>
        + RelativeEncodableKnownSize<PersonalPrivateInterest<MCL, MCC, MPL, N, S>>,
{
    fn relative_len_of_encoding(&self, r: &PersonalPrivateInterest<MCL, MCC, MPL, N, S>) -> usize {
        let sender_handle_tag =
            compact_u64::Tag::min_tag(self.sender_handle, compact_u64::TagWidth::two());
        let receiver_handle_tag =
            compact_u64::Tag::min_tag(self.receiver_handle, compact_u64::TagWidth::two());
        let max_count_tag = compact_u64::Tag::min_tag(self.max_count, compact_u64::TagWidth::two());
        let max_size_tag = compact_u64::Tag::min_tag(self.max_size, compact_u64::TagWidth::two());

        let sender_handle_len = CompactU64(self.sender_handle)
            .relative_len_of_encoding(&sender_handle_tag.encoding_width());

        let receiver_handle_len = CompactU64(self.receiver_handle)
            .relative_len_of_encoding(&receiver_handle_tag.encoding_width());

        let max_count_len =
            CompactU64(self.max_count).relative_len_of_encoding(&max_count_tag.encoding_width());

        let max_size_len =
            CompactU64(self.max_size).relative_len_of_encoding(&max_size_tag.encoding_width());

        let cap_len = self.capability.relative_len_of_encoding(r);

        1 + sender_handle_len + receiver_handle_len + max_count_len + max_size_len + 1 + cap_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, RC>
    RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>
    for PioBindReadCapability<MCL, MCC, MPL, RC>
where
    N: NamespaceId,
    S: SubspaceId,
    RC: RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, S>, Blame>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
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

        let capability = RC::relative_decode(producer, r).await?;

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

/// The data shared between ReconciliationSendFingerprint and ReconciliationAnnounceEntries messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RangeInfo<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// The count of the message this message is responding to. If this message was not created in response to any other message, this field is 0.
    pub covers: u64,
    /// Whether this message is the final message sent in response to some other message. If this message is not sent as a response at all, this field is true.
    pub is_final: bool,
    /// The 3dRange the message pertains to.
    pub range: Range3d<MCL, MCC, MPL, S>,
    /// A ReadCapabilityHandle bound by the sender of this message. The granted area of the corresponding read capability must fully the range.
    pub sender_handle: u64,
    /// A ReadCapabilityHandle bound by the receiver of this message. The granted area of the corresponding read capability must fully contain the range.
    pub receiver_handle: u64,
}

/// Send a Fingerprint as part of 3d range-based set reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationSendFingerprint<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S,
    Fingerprint,
> {
    /// The RangeInfo for this message.
    pub info: RangeInfo<MCL, MCC, MPL, S>,
    /// The Fingerprint of all LengthyAuthorisedEntries the peer has in info.range.
    pub fingerprint: Fingerprint,
}

/// Prepare transmission of the LengthyAuthorisedEntries a peer has in a 3dRange as part of 3d range-based set reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationAnnounceEntries<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    S,
> {
    /// The RangeInfo for this message.
    pub info: RangeInfo<MCL, MCC, MPL, S>,
    /// Must be true if and only if the the sender has zero Entries in info.range.
    pub is_empty: bool,
    /// A boolean flag to indicate whether the sender wishes to receive a ReconciliationAnnounceEntries message for the same 3dRange in return.
    pub want_response: bool,
    /// Whether the sender promises to send the Entries in info.range sorted ascendingly by subspace_id , using paths (sorted lexicographically) as the tiebreaker.
    pub will_sort: bool,
}

/// Send a LengthyAuthorisedEntry as part of 3d range-based set reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationSendEntry<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
> {
    /// The LengthyAuthorisedEntry itself.
    pub entry: LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    /// The index of the first (transformed) Payload Chunk that will be transmitted for entry. Set this to the total number of Chunks to indicate that no Chunks will be transmitted. In this case, the receiver must act as if it had received a ReconciliationTerminatePayload message immediately after this message.
    pub offset: u64,
}

/// Send some Chunks as part of 3d range-based set reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationSendPayload {
    /// The number of transmitted Chunks.
    pub amount: u64,
    /// The bytes to transmit, the concatenation of the Chunks obtained by applying transform_payload to the Payload of the receiver’s reconciliation_current_entryEntry, starting at the offset of the corresponding ReconciliationSendEntry message plus the number of Chunks for the current Entry that were already transmitted by prior ReconciliationSendPayload messages.
    pub bytes: Box<[u8]>,
}

/// Signal the end of the currentPayload transmission as part of 3d range-based set reconciliation, and indicate whether another LengthyAuthorisedEntry transmission will follow for the current 3dRange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationTerminatePayload {
    /// Set to true if and only if no further ReconciliationSendEntry message will be sent as part of reconciling the current 3dRange.
    pub is_final: bool,
}
