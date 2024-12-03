// TODO implement Encodable for each of these (but not for `IncomingFragmentHeader`)

pub struct IssueGuarantee {
    pub channel: u64,
    pub amount: u64,
}

pub struct Absolve {
    pub channel: u64,
    pub amount: u64,
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

pub struct Apologise {
    pub channel: u64,
}

/// Does not include the actual message bytes.
pub struct SendToChannelHeader {
    pub channel: u64,
    pub length: u64,
}

/// Does not include the actual message bytes.
pub struct SendControlHeader {
    /// Information stored in the four least significant bits.
    pub encoding_nibble: u8,
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