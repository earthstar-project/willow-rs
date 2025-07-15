#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::{CompactU64, TagWidth};

use crate::pio::EnumerationCapability;
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable, RelativeEncodableKnownSize,
};
use willow_encoding::is_bitflagged;

pub(crate) enum GlobalMessage {
    ResourceHandleFree(ResourceHandleFree),
    DataSetEagerness(DataSetEagerness),
    // TODO: Also PioAnnounceOverlap, which requires generics
}

impl Encodable for GlobalMessage {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl Decodable for GlobalMessage {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

pub(crate) struct ResourceHandleFree {}

impl Encodable for ResourceHandleFree {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl Decodable for ResourceHandleFree {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

pub(crate) struct DataSetEagerness {}

impl Encodable for DataSetEagerness {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl Decodable for DataSetEagerness {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
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
    hash: [u8; INTEREST_HASH_LENGTH],

    /// Whether the peer is directly interested in the hashed PrivateInterest, or whether it is merely a relaxation.
    actually_interested: bool,
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
    sender_handle: u64,

    /// The OverlapHandle (bound by the receiver of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    receiver_handle: u64,

    /// The announcement authentication for this overlap announcement.
    authentication: [u8; INTEREST_HASH_LENGTH],

    /// The enumeration capability if this overlap announcement is for an awkward pair, or none otherwise.
    pub enumeration_capability: Option<EnumerationCapability>,
}

impl<const INTEREST_HASH_LENGTH: usize, N, R, EC> RelativeEncodable<(N, R)>
    for PioAnnounceOverlap<INTEREST_HASH_LENGTH, EC>
where
    N: PartialEq,
    R: PartialEq,
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
                enumeration_capability.granted_namespace(),
                enumeration_capability.receiver(),
            );

            assert!(r == &pair);

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
    N: PartialEq,
    R: PartialEq,
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
    EC: EnumerationCapability<Receiver = R, NamespaceId = N> + RelativeDecodable<(N, R), Blame>,
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

// TODO: ReadCapability param should use the actual trait.
/// Bind a read capability for an overlap between two PrivateInterests. Additionally, this message specifies an AreaOfInterest which the sender wants to sync.
struct PioBindReadCapability<ReadCapability> {
    /// The OverlapHandle (bound by the sender of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    sender_handle: u64,

    /// The OverlapHandle (bound by the receiver of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    receiver_handle: u64,

    /// The ReadCapability to bind. Its granted namespace must be the (shared) namespace_id of the two PrivateInterests. Its granted area must be included in the less specific of the two PrivateInterests.
    capability: ReadCapability,

    /// The max_count of the AreaOfInterest that the sender wants to sync.
    max_count: u64,

    /// The max_size of the AreaOfInterest that the sender wants to sync.
    max_size: u64,

    /// When the receiver of this message eagerly transmits Entries for the AreaOfInterest defined by this message, it must not include the Payload of Entries whose payload_length is strictly greater than two to the power of the max_payload_power. We call the resulting number the sender’s maximum payload size for this AreaOfInterest.
    max_payload_power: u8,
}
