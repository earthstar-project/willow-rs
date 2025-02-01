// TODO implement Encodable for each of these (but not for `IncomingFrameHeader`)

use compact_u64::{CompactU64, Tag, TagWidth};
use ufotofu::BulkConsumer;
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, RelativeDecodable, RelativeEncodable,
};

pub struct IssueGuarantee {
    pub channel: u64,
    pub amount: u64,
}

impl Encodable for IssueGuarantee {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // Header with compacted channel u64
        HeaderWithEmbeddedChannelAndMaybeLength::IssueGuarantee(self.channel)
            .encode(consumer)
            .await?;

        // Amount
        MessageFieldU64(self.amount).encode(consumer).await?;

        Ok(())
    }
}

pub struct Absolve {
    pub channel: u64,
    pub amount: u64,
}

impl Encodable for Absolve {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // Header with compacted channel u64
        HeaderWithEmbeddedChannelAndMaybeLength::Absolve(self.channel)
            .encode(consumer)
            .await?;

        // Amount
        MessageFieldU64(self.amount).encode(consumer).await?;

        Ok(())
    }
}

pub struct Plead {
    pub channel: u64,
    pub target: u64,
}

impl Encodable for Plead {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // Header with compacted channel u64
        HeaderWithEmbeddedChannelAndMaybeLength::Plead(self.channel)
            .encode(consumer)
            .await?;

        // Target
        MessageFieldU64(self.target).encode(consumer).await?;

        Ok(())
    }
}

pub struct LimitSending {
    pub channel: u64,
    pub bound: u64,
}

impl Encodable for LimitSending {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // Header with compacted channel u64
        HeaderWithEmbeddedChannelAndMaybeLength::LimitSending(self.channel)
            .encode(consumer)
            .await?;

        // Bound
        MessageFieldU64(self.bound).encode(consumer).await?;

        Ok(())
    }
}

pub struct LimitReceiving {
    pub channel: u64,
    pub bound: u64,
}

impl Encodable for LimitReceiving {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // Header with compacted channel u64
        HeaderWithEmbeddedChannelAndMaybeLength::LimitReceiving(self.channel)
            .encode(consumer)
            .await?;

        // Bound
        MessageFieldU64(self.bound).encode(consumer).await?;

        Ok(())
    }
}

pub struct AnnounceDropping {
    pub channel: u64,
}

impl Encodable for AnnounceDropping {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        HeaderWithEmbeddedChannelAndMaybeLength::AnnounceDropping(self.channel)
            .encode(consumer)
            .await?;

        Ok(())
    }
}

pub struct Apologise {
    pub channel: u64,
}

impl Encodable for Apologise {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        HeaderWithEmbeddedChannelAndMaybeLength::Apologise(self.channel)
            .encode(consumer)
            .await?;

        Ok(())
    }
}

/// Does not include the actual message bytes.
pub struct SendToChannelHeader {
    pub channel: u64,
    pub length: u64,
}

/// A byte whose most significant four bits are zero, and whose least significant four bits contain encoding information for a SendControl frame.
pub type SendControlNibble = u8;
impl Encodable for SendToChannelHeader {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((self.length, self.channel))
            .encode(consumer)
            .await?;

        Ok(())
    }
}

/// Does not include the actual message bytes.
pub struct SendControlHeader {
    /// Information stored in the four least significant bits.
    pub encoding_nibble: SendControlNibble,
}

/// An incoming LCMUX frame header: all information, except for the message bytes in case of a `SendToChannel` or `SendControl` frame.
impl Encodable for SendControlHeader {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let header = 0b1000_0000 | self.encoding_nibble;

        consumer.consume(header).await?;

        Ok(())
    }
}

/// An incoming LCMUX fragment header: all information, except for the message bytes in case of a `SendToChannel` or `SendControl` fragment.
///
/// Implements [`Decodable`] because we use this to figure out with incoming data. Does not implement [`Encodable`], however, since we already know which kind of header we are encoding.
pub enum IncomingFrameHeader {
    IssueGuarantee(IssueGuarantee),
    Absolve(Absolve),
    Plead(Plead),
    LimitSending(LimitSending),
    LimitReceiving(LimitReceiving),
    AnnounceDropping(AnnounceDropping),
    Apologise(Apologise),
    SendToChannelHeader(SendToChannelHeader),
    SendControlHeader(SendControlHeader),
}

impl Decodable for IncomingFrameHeader {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let id = header & 0b1111_0000;

        match id {
            0b1000_0000 => {
                // Decode nibble
                let nibble = 0b0000_1111 & header;

                Ok(IncomingFrameHeader::SendControlHeader(SendControlHeader {
                    encoding_nibble: nibble,
                }))
            }
            _ => {
                let header = HeaderWithEmbeddedChannelAndMaybeLength::decode(producer).await?;

                // And then get the message field.
                match header {
                    HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((length, channel)) => {
                        Ok(IncomingFrameHeader::SendToChannelHeader(
                            SendToChannelHeader { length, channel },
                        ))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::IssueGuarantee(channel) => {
                        let field = MessageFieldU64::decode(producer).await?.0;

                        Ok(IncomingFrameHeader::IssueGuarantee(IssueGuarantee {
                            channel,
                            amount: field,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::Absolve(channel) => {
                        let field = MessageFieldU64::decode(producer).await?.0;

                        Ok(IncomingFrameHeader::Absolve(Absolve {
                            channel,
                            amount: field,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::Plead(channel) => {
                        let field = MessageFieldU64::decode(producer).await?.0;

                        Ok(IncomingFrameHeader::Plead(Plead {
                            channel,
                            target: field,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::LimitSending(channel) => {
                        let field = MessageFieldU64::decode(producer).await?.0;

                        Ok(IncomingFrameHeader::LimitSending(LimitSending {
                            channel,
                            bound: field,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::LimitReceiving(channel) => {
                        let field = MessageFieldU64::decode(producer).await?.0;

                        Ok(IncomingFrameHeader::LimitReceiving(LimitReceiving {
                            channel,
                            bound: field,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::AnnounceDropping(channel) => {
                        Ok(IncomingFrameHeader::AnnounceDropping(AnnounceDropping {
                            channel,
                        }))
                    }
                    HeaderWithEmbeddedChannelAndMaybeLength::Apologise(channel) => {
                        Ok(IncomingFrameHeader::Apologise(Apologise { channel }))
                    }
                }
            }
        }
    }
}

struct MessageFieldU64(u64);

impl Encodable for MessageFieldU64 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        CompactU64(self.0)
            .relative_encode(consumer, &compact_u64::EncodingWidth::eight())
            .await?;

        Ok(())
    }
}

impl Decodable for MessageFieldU64 {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let tag = Tag::min_tag(0, TagWidth::eight());

        let value = CompactU64::relative_decode(producer, &tag)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(MessageFieldU64(value.0))
    }
}

/// Represents the header byte of LCMUX messages which include a logical channel u64 with them
enum HeaderWithEmbeddedChannelAndMaybeLength {
    // First u64 is the length field of `SendToChannel`, second is channel
    SendToChannel((u64, u64)),
    IssueGuarantee(u64),
    Absolve(u64),
    Plead(u64),
    LimitSending(u64),
    LimitReceiving(u64),
    AnnounceDropping(u64),
    Apologise(u64),
}

impl HeaderWithEmbeddedChannelAndMaybeLength {
    fn header_id_bits(&self) -> u8 {
        match self {
            HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((length, _)) => {
                if *length < 256 {
                    0b0000_0000
                } else if *length < 256_u64.pow(2) {
                    0b0001_0000
                } else if *length < 256_u64.pow(3) {
                    0b0010_0000
                } else if *length < 256_u64.pow(4) {
                    0b0011_0000
                } else if *length < 256_u64.pow(5) {
                    0b0100_0000
                } else if *length < 256_u64.pow(6) {
                    0b0101_0000
                } else if *length < 256_u64.pow(7) {
                    0b0110_0000
                } else {
                    0b0111_0000
                }
            }
            HeaderWithEmbeddedChannelAndMaybeLength::IssueGuarantee(_) => 0b1001_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::Absolve(_) => 0b1010_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::Plead(_) => 0b1011_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::LimitSending(_) => 0b1100_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::LimitReceiving(_) => 0b1101_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::AnnounceDropping(_) => 0b1110_0000,
            HeaderWithEmbeddedChannelAndMaybeLength::Apologise(_) => 0b1111_0000,
        }
    }
}

impl Encodable for HeaderWithEmbeddedChannelAndMaybeLength {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        // So what if aljoscha's going to shout at me,
        let channel = match self {
            HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((_, c)) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::IssueGuarantee(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::Absolve(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::Plead(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::LimitSending(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::LimitReceiving(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::AnnounceDropping(c) => *c,
            HeaderWithEmbeddedChannelAndMaybeLength::Apologise(c) => *c,
        };

        let tag = Tag::min_tag(channel, TagWidth::four());
        let header = self.header_id_bits() | tag.data();
        consumer.consume(header).await?;
        CompactU64(channel)
            .relative_encode(consumer, &tag.encoding_width())
            .await?;

        if let HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((length, _)) = self {
            // Encode the length in only as many bytes as we need...
            let length_bytes = length.to_be_bytes();

            let bytes_to_consume = min_bytes_needed(*length) as usize;

            let slice = &length_bytes[8 - bytes_to_consume..];

            consumer
                .consume_full_slice(slice)
                .await
                .map_err(|err| err.into_reason())?;
        }

        Ok(())
    }
}

const fn min_bytes_needed(n: u64) -> u8 {
    if n < 256 {
        1
    } else if n < 256_u64.pow(2) {
        2
    } else if n < 256_u64.pow(3) {
        3
    } else if n < 256_u64.pow(4) {
        4
    } else if n < 256_u64.pow(5) {
        5
    } else if n < 256_u64.pow(6) {
        6
    } else if n < 256_u64.pow(7) {
        7
    } else {
        8
    }
}

impl Decodable for HeaderWithEmbeddedChannelAndMaybeLength {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let first_byte = producer.produce_item().await?;

        let tag = Tag::from_raw(first_byte, TagWidth::four(), 4);

        let channel = CompactU64::relative_decode(producer, &tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0 as u64;

        // Decode the channel appropriately

        match 0b1111_0000 & first_byte {
            // This is a `SendControlMessage`, which we really should have not let get this far.
            0b1000_0000 => Err(DecodeError::Other(Blame::OurFault)),
            0b1001_0000 => {
                // Return IssueGuarantee with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::IssueGuarantee(
                    channel,
                ))
            }
            0b1010_0000 => {
                // Return Absolve with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::Absolve(channel))
            }
            0b1011_0000 => {
                // Return Plead with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::Plead(channel))
            }
            0b1100_0000 => {
                // Return LimitSending with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::LimitSending(
                    channel,
                ))
            }
            0b1101_0000 => {
                // Return LimitReceiving with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::LimitReceiving(
                    channel,
                ))
            }
            0b1110_0000 => {
                // Return AnnounceDropping with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::AnnounceDropping(
                    channel,
                ))
            }
            0b1111_0000 => {
                // Return Apologise with decoded channel
                Ok(HeaderWithEmbeddedChannelAndMaybeLength::Apologise(channel))
            }
            _ => {
                // This is a SendToChannel!

                // Decode the length... width
                let bytes_to_produce = ((0b0111_0000 & first_byte) >> 4) as usize;

                let mut length_bytes = [0_u8; 8];

                let slice = &mut length_bytes[8 - bytes_to_produce..];

                producer.overwrite_full_slice(slice).await?;

                Ok(HeaderWithEmbeddedChannelAndMaybeLength::SendToChannel((
                    u64::from_be_bytes(length_bytes),
                    channel,
                )))
            }
        }
    }
}
