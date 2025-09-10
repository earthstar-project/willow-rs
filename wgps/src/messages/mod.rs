use std::rc::Rc;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::{CompactU64, Tag, TagWidth};
use either::Either::{Left, Right};
use willow_data_model::{
    grouping::Range3d, AuthorisedEntry, Entry, LengthyAuthorisedEntry, NamespaceId, Path,
    PayloadDigest, SubspaceId,
};

use crate::{
    parameters::{EnumerationCapability, ReadCapability},
    pio,
};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeEncodable, RelativeEncodableKnownSize,
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
    // Indicates the root message id of the prior root message this message refers to (when set to a non-zero U64), or indicates that this message is a fresh root message itself (when set to 0).
    pub root_id: u64,
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

impl<const MCL: usize, const MCC: usize, const MPL: usize, S, FP>
    RelativeEncodable<Range3d<MCL, MCC, MPL, S>>
    for ReconciliationSendFingerprint<MCL, MCC, MPL, S, FP>
where
    S: SubspaceId + Encodable,
    FP: Encodable,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0b0000_0000;

        let root_id_tag = Tag::min_tag(self.info.root_id, TagWidth::four());
        let sender_handle_tag = Tag::min_tag(self.info.sender_handle, TagWidth::two());
        let receiver_handle_tag = Tag::min_tag(self.info.receiver_handle, TagWidth::two());

        header |= root_id_tag.data_at_offset(0);
        header |= sender_handle_tag.data_at_offset(4);
        header |= receiver_handle_tag.data_at_offset(6);

        consumer.consume(header).await?;

        CompactU64(self.info.root_id)
            .relative_encode(consumer, &root_id_tag.encoding_width())
            .await?;
        CompactU64(self.info.sender_handle)
            .relative_encode(consumer, &sender_handle_tag.encoding_width())
            .await?;
        CompactU64(self.info.receiver_handle)
            .relative_encode(consumer, &receiver_handle_tag.encoding_width())
            .await?;

        self.info.range.relative_encode(consumer, r).await?;

        self.fingerprint.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S, FP>
    RelativeEncodableKnownSize<Range3d<MCL, MCC, MPL, S>>
    for ReconciliationSendFingerprint<MCL, MCC, MPL, S, FP>
where
    S: SubspaceId + EncodableKnownSize,
    FP: EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Range3d<MCL, MCC, MPL, S>) -> usize {
        let root_id_tag = Tag::min_tag(self.info.root_id, TagWidth::four());
        let sender_handle_tag = Tag::min_tag(self.info.sender_handle, TagWidth::two());
        let receiver_handle_tag = Tag::min_tag(self.info.receiver_handle, TagWidth::two());

        let root_id_len =
            CompactU64(self.info.root_id).relative_len_of_encoding(&root_id_tag.encoding_width());
        let sender_handle_len = CompactU64(self.info.sender_handle)
            .relative_len_of_encoding(&sender_handle_tag.encoding_width());
        let receiver_handle_len = CompactU64(self.info.receiver_handle)
            .relative_len_of_encoding(&receiver_handle_tag.encoding_width());

        let range_rel_len = self.info.range.relative_len_of_encoding(r);

        let fp_len = self.fingerprint.len_of_encoding();

        1 + root_id_len + sender_handle_len + receiver_handle_len + range_rel_len + fp_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S, FP>
    RelativeDecodable<Range3d<MCL, MCC, MPL, S>, Blame>
    for ReconciliationSendFingerprint<MCL, MCC, MPL, S, FP>
where
    S: SubspaceId + DecodableCanonic,
    FP: Decodable,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic> + From<FP::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let root_id_tag = Tag::from_raw(header, TagWidth::four(), 0);
        let sender_handle_tag = Tag::from_raw(header, TagWidth::two(), 4);
        let receiver_handle_tag = Tag::from_raw(header, TagWidth::two(), 6);

        let root_id = CompactU64::relative_decode(producer, &root_id_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let sender_handle = CompactU64::relative_decode(producer, &sender_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let receiver_handle = CompactU64::relative_decode(producer, &receiver_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let range = Range3d::relative_decode(producer, r)
            .await
            .map_err(DecodeError::map_other_from)?;

        let fingerprint = FP::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(Self {
            fingerprint,
            info: RangeInfo {
                root_id,
                sender_handle,
                receiver_handle,
                range,
            },
        })
    }
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

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodable<Range3d<MCL, MCC, MPL, S>> for ReconciliationAnnounceEntries<MCL, MCC, MPL, S>
where
    S: SubspaceId + Encodable,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header_1 = 0b0000_0000;

        if self.is_empty {
            header_1 |= 0b0001_0000;
        }

        if self.want_response {
            header_1 |= 0b0000_1000;
        }

        if self.will_sort {
            header_1 |= 0b0000_0100;
        }

        let root_id_tag = Tag::min_tag(self.info.root_id, TagWidth::two());

        header_1 |= root_id_tag.data_at_offset(5);

        consumer.consume(header_1).await?;

        let mut header_2 = 0b0000_0000;

        let sender_handle_tag = Tag::min_tag(self.info.sender_handle, TagWidth::four());
        let receiver_handle_tag = Tag::min_tag(self.info.receiver_handle, TagWidth::four());

        header_2 |= sender_handle_tag.data_at_offset(0);
        header_2 |= receiver_handle_tag.data_at_offset(4);

        consumer.consume(header_2).await?;

        CompactU64(self.info.root_id)
            .relative_encode(consumer, &root_id_tag.encoding_width())
            .await?;
        CompactU64(self.info.sender_handle)
            .relative_encode(consumer, &sender_handle_tag.encoding_width())
            .await?;
        CompactU64(self.info.receiver_handle)
            .relative_encode(consumer, &receiver_handle_tag.encoding_width())
            .await?;

        self.info.range.relative_encode(consumer, r).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeEncodableKnownSize<Range3d<MCL, MCC, MPL, S>>
    for ReconciliationAnnounceEntries<MCL, MCC, MPL, S>
where
    S: SubspaceId + EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, r: &Range3d<MCL, MCC, MPL, S>) -> usize {
        let root_id_tag = Tag::min_tag(self.info.root_id, TagWidth::two());
        let sender_handle_tag = Tag::min_tag(self.info.sender_handle, TagWidth::four());
        let receiver_handle_tag = Tag::min_tag(self.info.receiver_handle, TagWidth::four());

        let root_id_len =
            CompactU64(self.info.root_id).relative_len_of_encoding(&root_id_tag.encoding_width());
        let sender_handle_len = CompactU64(self.info.sender_handle)
            .relative_len_of_encoding(&sender_handle_tag.encoding_width());
        let receiver_handle_len = CompactU64(self.info.receiver_handle)
            .relative_len_of_encoding(&receiver_handle_tag.encoding_width());

        let range_rel_len = self.info.range.relative_len_of_encoding(r);

        2 + root_id_len + sender_handle_len + receiver_handle_len + range_rel_len
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S>
    RelativeDecodable<Range3d<MCL, MCC, MPL, S>, Blame>
    for ReconciliationAnnounceEntries<MCL, MCC, MPL, S>
where
    S: SubspaceId + DecodableCanonic,
    Blame: From<S::ErrorReason> + From<S::ErrorCanonic>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Range3d<MCL, MCC, MPL, S>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header_1 = producer.produce_item().await?;

        let is_empty = is_bitflagged(header_1, 3);
        let want_response = is_bitflagged(header_1, 4);
        let will_sort = is_bitflagged(header_1, 5);
        let root_id_tag = Tag::from_raw(header_1, TagWidth::two(), 5);

        let header_2 = producer.produce_item().await?;

        let sender_handle_tag = Tag::from_raw(header_2, TagWidth::four(), 0);
        let receiver_handle_tag = Tag::from_raw(header_2, TagWidth::four(), 4);

        let root_id = CompactU64::relative_decode(producer, &root_id_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let sender_handle = CompactU64::relative_decode(producer, &sender_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let receiver_handle = CompactU64::relative_decode(producer, &receiver_handle_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let range = Range3d::relative_decode(producer, r)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(Self {
            is_empty,
            want_response,
            will_sort,
            info: RangeInfo {
                root_id,
                sender_handle,
                receiver_handle,
                range,
            },
        })
    }
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

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    RelativeEncodable<(
        &(N, Range3d<MCL, MCC, MPL, S>),
        &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    )> for ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId + Encodable + EncodableKnownSize,
    S: SubspaceId + Encodable + EncodableKnownSize,
    PD: PayloadDigest + Encodable + EncodableKnownSize,
    AT: RelativeEncodable<(
        AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        Entry<MCL, MCC, MPL, N, S, PD>,
    )>,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(
            &(N, Range3d<MCL, MCC, MPL, S>),
            &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        ),
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let (r_range, r_authed) = r;

        // Determine whether we are going to encode relative_to_entry or not.
        let rel_entry_len = self
            .entry
            .entry()
            .entry()
            .relative_len_of_encoding(r_authed.entry());

        let rel_range_len = self
            .entry
            .entry()
            .entry()
            .relative_len_of_encoding(*r_range);

        let relative_to_entry = rel_entry_len < rel_range_len;

        let mut header = 0b0000_0000;

        if relative_to_entry {
            header |= 0b0001_0000;
        }

        let offset_tag = Tag::min_tag(self.offset, TagWidth::two());
        let available_tag = Tag::min_tag(self.entry.available(), TagWidth::two());

        header |= offset_tag.data_at_offset(4);
        header |= offset_tag.data_at_offset(6);

        consumer.consume(header).await?;

        CompactU64(self.offset)
            .relative_encode(consumer, &offset_tag.encoding_width())
            .await?;

        CompactU64(self.entry.available())
            .relative_encode(consumer, &available_tag.encoding_width())
            .await?;

        if relative_to_entry {
            self.entry
                .entry()
                .entry()
                .relative_encode(consumer, r_authed.entry())
                .await?;
        } else {
            self.entry
                .entry()
                .entry()
                .relative_encode(consumer, *r_range)
                .await?;
        };

        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    RelativeEncodableKnownSize<(
        &Range3d<MCL, MCC, MPL, S>,
        &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    )> for ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    S: EncodableKnownSize,
{
    fn relative_len_of_encoding(
        &self,
        r: &(
            &Range3d<MCL, MCC, MPL, S>,
            &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        ),
    ) -> usize {
        todo!()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    RelativeDecodable<
        (
            &Range3d<MCL, MCC, MPL, S>,
            &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        ),
        Blame,
    > for ReconciliationSendEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    S: Decodable,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(
            &Range3d<MCL, MCC, MPL, S>,
            &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
        ),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

/// Send some Chunks as part of 3d range-based set reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationSendPayload {
    /// The number of transmitted Chunks.
    pub amount: u64,
}

impl Encodable for ReconciliationSendPayload {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!();
    }
}

impl EncodableKnownSize for ReconciliationSendPayload {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for ReconciliationSendPayload {}

impl Decodable for ReconciliationSendPayload {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
    {
        todo!()
    }
}

/// Signal the end of the currentPayload transmission as part of 3d range-based set reconciliation, and indicate whether another LengthyAuthorisedEntry transmission will follow for the current 3dRange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconciliationTerminatePayload {
    /// Set to true if and only if no further ReconciliationSendEntry message will be sent as part of reconciling the current 3dRange.
    pub is_final: bool,
}

impl Encodable for ReconciliationTerminatePayload {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!();
    }
}

impl EncodableKnownSize for ReconciliationTerminatePayload {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for ReconciliationTerminatePayload {}

impl Decodable for ReconciliationTerminatePayload {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
    {
        todo!()
    }
}

/// Transmit an AuthorisedEntry and set the receiver’s data_current_entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataSendEntry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
{
    /// The AuthorisedEntry to transmit.
    pub entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    /// The index of the first (transformed) Payload Chunk that will be transmitted for entry. Can be set arbitrarily if no Chunks will be transmitted, should be set to 0 in that case.
    pub offset: u64,
}

/// Send some Chunks of the receiver’s data_current_entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataSendPayload {
    /// The number of transmitted Chunks.
    pub amount: u64,
}

/// Express eagerness preferences for the Payload transmissions in the overlaps of the granted areas of two ReadCapabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataSetEagerness {
    // A ReadCapabilityHandle bound by the sender of this message. This message pertains to the granted area of the corresponding read capability.
    pub sender_handle: u64,
    // A ReadCapabilityHandle bound by the receiver of this message. This message pertains to the granted area of the corresponding read capability.
    pub receiver_handle: u64,
    // Whether the receiver should eagerly include Payloads when it pushes Entries from the overlap of the granted areas of the ReadCapability corresponding to sender_handle and receiver_handle.
    pub set_eager: bool,
}

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

/// Bind a request for (parts of) a Payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PayloadRequestBindRequest<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
> {
    // The namespace_id of the Entry whose Payload to request.
    namespace_id: N,
    // The subspace_id of the Entry whose Payload to request.
    subspace_id: S,
    // The path of the Entry whose Payload to request.
    path: Path<MCL, MCC, MPL>,
    // The payload_digest of the Entry whose Payload to request.
    payload_digest: PD,
    // A ReadCapabilityHandle bound by the sender of this message. The granted area of the corresponding read capability must contain the namespace_id, subspace_id, and path.
    sender_handle: u64,
    // A ReadCapabilityHandle bound by the receiver of this message. The granted area of the corresponding read capability must contain the namespace_id, subspace_id, and path.
    receiver_handle: u64,
}

/// Send some Chunks of a requested Entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PayloadRequestSendResponse {
    // The PayloadRequestHandle of the request this is responding to.
    pub handle: u64,
    // The number of transmitted Chunks.
    pub amount: u64,
    // The bytes to transmit, the concatenation of the Chunks obtained by applying transform_payload to the Payload of the requestedEntry, starting at the requested offset plus the number of Chunks for the same request that were already transmitted by prior PayloadRequestSendResponse messages.
}

/// The different resource handles employed by the WGPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HandleType {
    // Resource handle for the hash-boolean pairs transmitted during private interest overlap detection.
    OverlapHandle,
    // Resource handle for ReadCapabilities that certify access to some Entries.
    ReadCapabilityHandle,
    // Resource handle for explicitly requesting (parts of) Payloads beyond what is exchanged automatically.
    PayloadRequestHandle,
}

/// Indicate that the sender wants to free a resource handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceHandleFree {
    // The type of resource handle to free.
    pub handle_type: HandleType,
    // Whether the resource handle to free was bound by the sender (true) or the receiver (false) of this message.
    pub mine: bool,
    // The numeric id of the resource handle to free.
    pub handle_id: u64,
    // The sender’s reference count for the resource handle to free.
    pub reference_count: u64,
}

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
