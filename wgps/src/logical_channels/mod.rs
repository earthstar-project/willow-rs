//! This module implements logical channels, as described at https://willowprotocol.org/specs/resource-control/index.html#resource_control
//! 
//! There is no implementation of optimistic sending, the client components fully respect the guarantees issued by the server side.

pub mod client;