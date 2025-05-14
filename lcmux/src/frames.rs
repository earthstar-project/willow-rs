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
        encode_header(0b1111_0000, self.channel, consumer).await?;
        CompactU64(self.amount).encode(consumer).await
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
        encode_header(0b1011_0000, self.channel, consumer).await?;
        CompactU64(self.amount).encode(consumer).await
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
        encode_header(0b1110_0000, self.channel, consumer).await?;
        CompactU64(self.target).encode(consumer).await
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
        encode_header(0b1010_0000, self.channel, consumer).await?;
        CompactU64(self.bound).encode(consumer).await
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
        encode_header(0b1101_0000, self.channel, consumer).await?;
        CompactU64(self.bound).encode(consumer).await
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
        encode_header(0b1100_0000, self.channel, consumer).await
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
        encode_header(0b1001_0000, self.channel, consumer).await
    }
}

/// Does not include the actual message bytes.
pub struct SendGlobalHeader {
    pub length: u64,
}

impl Encodable for SendGlobalHeader {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        encode_header(0b1000_0000, self.length, consumer).await
    }
}

/// Does not include the actual message bytes.
pub struct SendToChannelHeader {
    pub channel: u64,
    pub length: u64,
}

impl Encodable for SendToChannelHeader {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let len_tag = Tag::min_tag(self.length, TagWidth::three());
        let channel_tag = Tag::min_tag(self.channel, TagWidth::four());
        let first_byte = len_tag.data_at_offset(1) | channel_tag.data_at_offset(4);

        consumer.consume(first_byte).await?;
        CompactU64(self.channel)
            .relative_encode(consumer, &channel_tag.encoding_width())
            .await?;
        CompactU64(self.length)
            .relative_encode(consumer, &len_tag.encoding_width())
            .await
    }
}

/// An incoming LCMUX fragment header: all information, except for the message bytes in case of a `SendToChannel` or `SendGlobal` fragment.
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
    SendGlobalHeader(SendGlobalHeader),
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
        let first_byte = producer.produce_item().await?;

        if first_byte & 0b1000_0000 == 0 {
            // It's a SendChannelFrame!
            let len_tag = Tag::from_raw(first_byte, TagWidth::three(), 1);
            let channel_tag = Tag::from_raw(first_byte, TagWidth::four(), 4);

            let channel = CompactU64::relative_decode(producer, &channel_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0;
            let length = CompactU64::relative_decode(producer, &len_tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0;

            return Ok(IncomingFrameHeader::SendToChannelHeader(
                SendToChannelHeader { channel, length },
            ));
        } else {
            let tag = Tag::from_raw(first_byte, TagWidth::four(), 4);
            let val = CompactU64::relative_decode(producer, &tag)
                .await
                .map_err(DecodeError::map_other_from)?
                .0;

            match first_byte & 0b1111_0000 {
                0b1111_0000 => {
                    return Ok(IncomingFrameHeader::IssueGuarantee(IssueGuarantee {
                        channel: val,
                        amount: CompactU64::decode(producer)
                            .await
                            .map_err(DecodeError::map_other_from)?
                            .0,
                    }));
                }
                0b1110_0000 => {
                    return Ok(IncomingFrameHeader::Plead(Plead {
                        channel: val,
                        target: CompactU64::decode(producer)
                            .await
                            .map_err(DecodeError::map_other_from)?
                            .0,
                    }));
                }
                0b1101_0000 => {
                    return Ok(IncomingFrameHeader::LimitReceiving(LimitReceiving {
                        channel: val,
                        bound: CompactU64::decode(producer)
                            .await
                            .map_err(DecodeError::map_other_from)?
                            .0,
                    }));
                }
                0b1100_0000 => {
                    return Ok(IncomingFrameHeader::AnnounceDropping(AnnounceDropping {
                        channel: val,
                    }));
                }
                0b1011_0000 => {
                    return Ok(IncomingFrameHeader::Absolve(Absolve {
                        channel: val,
                        amount: CompactU64::decode(producer)
                            .await
                            .map_err(DecodeError::map_other_from)?
                            .0,
                    }));
                }
                0b1010_0000 => {
                    return Ok(IncomingFrameHeader::LimitSending(LimitSending {
                        channel: val,
                        bound: CompactU64::decode(producer)
                            .await
                            .map_err(DecodeError::map_other_from)?
                            .0,
                    }));
                }
                0b1001_0000 => {
                    return Ok(IncomingFrameHeader::Apologise(Apologise { channel: val }));
                }
                0b1000_0000 => {
                    return Ok(IncomingFrameHeader::SendGlobalHeader(SendGlobalHeader {
                        length: val,
                    }));
                }
                _ => unreachable!(),
            }
        }
    }
}

/// Most frame encodings start with a header consisting of a four bit discrimant, a four bit C64 tag, and the C64. This function encodes that header.
/// The discriminant must be stored in the four most significant bits of the argument, the other four bits must be zero.
async fn encode_header<C: BulkConsumer<Item = u8>>(
    discriminant: u8,
    data: u64,
    consumer: &mut C,
) -> Result<(), C::Error> {
    let tag = Tag::min_tag(data, TagWidth::four());
    let first_byte = tag.data_at_offset(4) | discriminant;
    consumer.consume(first_byte).await?;
    CompactU64(data)
        .relative_encode(consumer, &tag.encoding_width())
        .await
}
