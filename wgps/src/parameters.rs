use std::hash::Hash;

use ufotofu_codec::{
    Blame, DecodableCanonic, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodableKnownSize,
};
use willow_data_model::{grouping::Area, NamespaceId, SubspaceId};
use willow_pio::PersonalPrivateInterest;

pub trait WgpsNamespaceId: NamespaceId + Hash {}

pub trait WgpsSubspaceId:
    SubspaceId + Hash + Default + EncodableSync + EncodableKnownSize + DecodableCanonic + Clone
{
}

/// The semantics a valid read capability must provide to be usable with the WGPS.
pub trait ReadCapability<const MCL: usize, const MCC: usize, const MPL: usize> {
    type NamespaceId;
    type SubspaceId;

    fn granted_area(&self) -> &Area<MCL, MCC, MPL, Self::SubspaceId>;
    fn granted_namespace(&self) -> &Self::NamespaceId;
}

/// The semantics a valid read capability must provide to be usable with the WGPS, together with other trait bounds it has to satisfy in order to work with our specific WGPS implementation.
pub trait WgpsReadCapability<const MCL: usize, const MCC: usize, const MPL: usize>:
    ReadCapability<MCL, MCC, MPL>
    + RelativeDecodable<
        PersonalPrivateInterest<MCL, MCC, MPL, Self::NamespaceId, Self::SubspaceId>,
        Blame,
    > + Hash
    + Eq
    + Clone
    + RelativeEncodableKnownSize<
        PersonalPrivateInterest<MCL, MCC, MPL, Self::NamespaceId, Self::SubspaceId>,
    > + 'static
{
}

/// The semantics a valid enumeration capability must provide to be usable with the WGPS.
pub trait EnumerationCapability {
    type NamespaceId;
    type Receiver;

    fn granted_namespace(&self) -> &Self::NamespaceId;
    fn receiver(&self) -> &Self::Receiver;
}

/// The semantics a valid enumeration capability must provide to be usable with the WGPS, together with other trait bounds it has to satisfy in order to work with our specific WGPS implementation.
pub trait WgpsEnumerationCapability:
    EnumerationCapability
    + Clone
    + Eq
    + Hash
    + RelativeEncodableKnownSize<(Self::NamespaceId, Self::Receiver)>
    + RelativeDecodable<(Self::NamespaceId, Self::Receiver), Blame>
    + 'static
{
}
