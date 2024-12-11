// TODO implement Encodable for each of these (but not for `IncomingFragmentHeader`)

use ufotofu::BulkConsumer;
use ufotofu_codec::{Encodable, RelativeEncodable};

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
        HeaderWithEmbeddedChannel::IssueGuarantee
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::Absolve
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::Plead
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::LimitSending
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::LimitReceiving
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::AnnounceDropping
            .relative_encode(consumer, &self.channel)
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
        HeaderWithEmbeddedChannel::Apologise
            .relative_encode(consumer, &self.channel)
            .await?;

        Ok(())
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
        HeaderWithEmbeddedChannel::SendToChannel(self.length)
            .relative_encode(consumer, &self.channel)
            .await?;

        // Encode the length in only as many bytes as we need...
        let length_bytes = self.length.to_be_bytes();

        // Aljoscha I didn't sleep last night, leave me alone
        let bytes_to_consume = if self.length < 256 {
            1
        } else if self.length < 256_u64.pow(2) {
            2
        } else if self.length < 256_u64.pow(3) {
            3
        } else if self.length < 256_u64.pow(4) {
            4
        } else if self.length < 256_u64.pow(5) {
            5
        } else if self.length < 256_u64.pow(6) {
            6
        } else if self.length < 256_u64.pow(7) {
            7
        } else {
            8
        };

        let slice = &length_bytes[8 - bytes_to_consume..];

        consumer
            .consume_full_slice(slice)
            .await
            .map_err(|err| err.into_reason())?;

        Ok(())
    }
}

/// Does not include the actual message bytes.
pub struct SendControlHeader {
    /// Information stored in the four least significant bits.
    pub encoding_nibble: u8,
}

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
pub enum IncomingFragmentHeader {
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

struct MessageFieldU64(u64);

impl Encodable for MessageFieldU64 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        if self.0 <= 251 {
            // Encode channel as 8 bit integer
            ufotofu_codec_endian::U8BE(self.0 as u8)
                .encode(consumer)
                .await?;
        } else if self.0 <= 255 {
            consumer.consume(252).await?;
            ufotofu_codec_endian::U8BE(self.0 as u8)
                .encode(consumer)
                .await?;
        } else if self.0 <= 2_u64.pow(16) {
            consumer.consume(253).await?;
            ufotofu_codec_endian::U16BE(self.0 as u16)
                .encode(consumer)
                .await?;
        } else if self.0 <= 2_u64.pow(32) {
            consumer.consume(254).await?;
            ufotofu_codec_endian::U32BE(self.0 as u32)
                .encode(consumer)
                .await?;
        } else {
            consumer.consume(255).await?;
            ufotofu_codec_endian::U64BE(self.0).encode(consumer).await?;
        }

        Ok(())
    }
}

/// Represents the header byte of LCMUX messages which include a logical channel u64 with them
enum HeaderWithEmbeddedChannel {
    SendToChannel(u64),
    IssueGuarantee,
    Absolve,
    Plead,
    LimitSending,
    LimitReceiving,
    AnnounceDropping,
    Apologise,
}

impl HeaderWithEmbeddedChannel {
    fn header_id(&self) -> u8 {
        match self {
            HeaderWithEmbeddedChannel::SendToChannel(length) => {
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
            HeaderWithEmbeddedChannel::IssueGuarantee => 0b1001_0000,
            HeaderWithEmbeddedChannel::Absolve => 0b1010_0000,
            HeaderWithEmbeddedChannel::Plead => 0b1011_0000,
            HeaderWithEmbeddedChannel::LimitSending => 0b1100_0000,
            HeaderWithEmbeddedChannel::LimitReceiving => 0b1101_0000,
            HeaderWithEmbeddedChannel::AnnounceDropping => 0b1110_0000,
            HeaderWithEmbeddedChannel::Apologise => 0b1111_0000,
        }
    }
}

impl RelativeEncodable<u64> for HeaderWithEmbeddedChannel {
    async fn relative_encode<C>(&self, consumer: &mut C, r: &u64) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let channel = *r;

        if channel <= 11 {
            let header = self.header_id() | channel as u8; // Directly encode the integer's least significant four bits
            consumer.consume(header).await?;
        } else if channel < 256 {
            let header = self.header_id() | 0b0000_1100; // Will be followed by another byte containing the integer
            consumer.consume(header).await?;

            // Encode channel as 8 bit integer
            ufotofu_codec_endian::U8BE(channel as u8)
                .encode(consumer)
                .await?;
        } else if channel < 256_u64.pow(2) {
            let header = self.header_id() | 0b0000_1101; // Will be followed by another two bytes containing the big-endian encoding of the integer
            consumer.consume(header).await?;

            // Encode channel as 16 bit integer
            ufotofu_codec_endian::U16BE(channel as u16)
                .encode(consumer)
                .await?;
        } else if channel < 256_u64.pow(4) {
            let header = self.header_id() | 0b0000_1110; // Will be followed by another four bytes containing the big-endian encoding of the integer
            consumer.consume(header).await?;

            // Encode channel as 32 bit integer
            ufotofu_codec_endian::U32BE(channel as u32)
                .encode(consumer)
                .await?;
        } else {
            let header = self.header_id() | 0b0000_1111; // Will be followed by another eight bytes containing the big-endian encoding of the integer
            consumer.consume(header).await?;

            // Encode channel as 64 bit integer
            ufotofu_codec_endian::U64BE(channel)
                .encode(consumer)
                .await?;
        };

        Ok(())
    }
}
