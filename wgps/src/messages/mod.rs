#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

use ufotofu_codec::{Blame, Decodable, Encodable, EncodableKnownSize, EncodableSync};
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
pub struct PioAnnounceOverlap<const INTEREST_HASH_LENGTH: usize, EnumerationCapability> {
    /// The OverlapHandle (bound by the sender of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    sender_handle: u64,

    /// The OverlapHandle (bound by the receiver of this message) which is part of the overlap. If there are two handles available, use the one that was bound with actually_interested == true.
    receiver_handle: u64,

    /// The announcement authentication for this overlap announcement.
    authentication: [u8; INTEREST_HASH_LENGTH],

    /// The enumeration capability if this overlap announcement is for an awkward pair, or none otherwise.
    enumeration_capability: Option<EnumerationCapability>,
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
