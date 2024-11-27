//! This module implements logical channels, as described at https://willowprotocol.org/specs/resource-control/index.html#resource_control
//!
//! There is no implementation of optimistic sending, the client components fully respect the guarantees issued by the server side.

// pub mod client;
pub mod server;

/// The different logical channels of the WGPS.
pub enum LogicalChannel {
    /// [https://willowprotocol.org/specs/sync/index.html#ReconciliationChannel]()
    Reconciliation,
    /// [https://willowprotocol.org/specs/sync/index.html#DataChannel](https://willowprotocol.org/specs/sync/index.html#DataChannel)
    Data,
    /// [https://willowprotocol.org/specs/sync/index.html#IntersectionChannel](https://willowprotocol.org/specs/sync/index.html#IntersectionChannel)
    Intersection,
    /// [https://willowprotocol.org/specs/sync/index.html#CapabilityChannel](https://willowprotocol.org/specs/sync/index.html#CapabilityChannel)
    Capability,
    /// [https://willowprotocol.org/specs/sync/index.html#AreaOfInterestChannel](https://willowprotocol.org/specs/sync/index.html#AreaOfInterestChannel)
    AreaOfInterest,
    /// [https://willowprotocol.org/specs/sync/index.html#PayloadRequestChannel](https://willowprotocol.org/specs/sync/index.html#PayloadRequestChannel)
    PayloadRequest,
    /// [https://willowprotocol.org/specs/sync/index.html#StaticTokenChannel](https://willowprotocol.org/specs/sync/index.html#StaticTokenChannel)
    StaticToken,
}

impl LogicalChannel {
    /// Maps a [`LogicalChannel`] to its three-bit encoding in the WGPS. The return valuues range from 0 to 6.
    ///
    /// Specification: https://willowprotocol.org/specs/sync/index.html#encode_channel
    pub fn encode_channel(&self) -> u8 {
        match self {
            LogicalChannel::Reconciliation => 0,
            LogicalChannel::Data => 1,
            LogicalChannel::Intersection => 2,
            LogicalChannel::Capability => 3,
            LogicalChannel::AreaOfInterest => 4,
            LogicalChannel::PayloadRequest => 5,
            LogicalChannel::StaticToken => 6,
        }
    }

    /// Constructs a [`LogicalChannel`] from the least significant three bits of the given byte. Returns `None` if all three bits are ones.
    ///
    /// More significant bits are ignored.
    ///
    /// Specification: this is the inverse of https://willowprotocol.org/specs/sync/index.html#encode_channel
    pub fn decode_channel(encoding: u8) -> Option<LogicalChannel> {
        match encoding & 0b0000_0111 {
            0 => Some(LogicalChannel::Reconciliation),
            1 => Some(LogicalChannel::Data),
            2 => Some(LogicalChannel::Intersection),
            3 => Some(LogicalChannel::Capability),
            4 => Some(LogicalChannel::AreaOfInterest),
            5 => Some(LogicalChannel::PayloadRequest),
            6 => Some(LogicalChannel::StaticToken),
            _ => None,
        }
    }
}
