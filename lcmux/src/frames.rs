// TODO implement Encodable for each of these (but not for `IncomingFragmentHeader`)

use ufotofu_codec::{Blame, Decodable, DecodeError, Encodable};

pub struct IssueGuarantee {
    pub channel: u64,
    pub amount: u64,
}

impl Encodable for IssueGuarantee {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

pub struct Absolve {
    pub channel: u64,
    pub amount: u64,
}

impl Encodable for Absolve {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

pub struct Plead {
    pub channel: u64,
    pub target: u64,
}

pub struct LimitSending {
    pub channel: u64,
    pub bound: u64,
}

pub struct LimitReceiving {
    pub channel: u64,
    pub bound: u64,
}

pub struct AnnounceDropping {
    pub channel: u64,
}

impl Encodable for AnnounceDropping {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

pub struct Apologise {
    pub channel: u64,
}

/// Does not include the actual message bytes.
pub struct SendToChannelHeader {
    pub channel: u64,
    pub length: u64,
}

/// A byte whose most significant four bits are zero, and whose least significant four bits contain encoding information for a SendControl frame.
pub type SendControlNibble = u8;

/// Does not include the actual message bytes.
pub struct SendControlHeader {
    /// Information stored in the four least significant bits.
    pub encoding_nibble: SendControlNibble,
}

impl Encodable for SendControlHeader {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

/// An incoming LCMUX frame header: all information, except for the message bytes in case of a `SendToChannel` or `SendControl` frame.
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
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
    {
        todo!()
    }
}
