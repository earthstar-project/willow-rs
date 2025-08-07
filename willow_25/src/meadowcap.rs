use crate::PayloadDigest25;
use crate::{NamespaceId25, Signature25, SubspaceId25};
use ufotofu_codec::Encodable;
use willow_data_model::NamespaceId;
use willow_sideload::SideloadAuthorisationToken;

pub type CommunalCapability =
    meadowcap::CommunalCapability<1024, 1024, 1024, NamespaceId25, SubspaceId25, Signature25>;

pub type CommunalHandover<'a> =
    meadowcap::CommunalHandover<'a, 1024, 1024, 1024, NamespaceId25, SubspaceId25, Signature25>;

pub type Delegation = meadowcap::Delegation<1024, 1024, 1024, SubspaceId25, Signature25>;

pub type AuthorisationToken = meadowcap::McAuthorisationToken<
    1024,
    1024,
    1024,
    NamespaceId25,
    Signature25,
    SubspaceId25,
    Signature25,
>;

impl SideloadAuthorisationToken<1024, 1024, 1024, NamespaceId25, SubspaceId25, PayloadDigest25>
    for AuthorisationToken
{
}

pub type SubspaceCapability =
    meadowcap::McSubspaceCapability<NamespaceId25, Signature25, SubspaceId25, Signature25>;

pub type OwnedCapability = meadowcap::OwnedCapability<
    1024,
    1024,
    1024,
    NamespaceId25,
    Signature25,
    SubspaceId25,
    Signature25,
>;

pub type OwnedHandover<'a> = meadowcap::OwnedHandover<
    'a,
    1024,
    1024,
    1024,
    NamespaceId25,
    Signature25,
    SubspaceId25,
    Signature25,
>;

pub type PersonalPrivateInterest =
    meadowcap::PersonalPrivateInterest<1024, 1024, 1024, NamespaceId25, SubspaceId25>;

pub type SubspaceDelegation =
    meadowcap::PersonalPrivateInterest<1024, 1024, 1024, NamespaceId25, SubspaceId25>;

pub type Capability = meadowcap::McCapability<
    1024,
    1024,
    1024,
    crate::NamespaceId25,
    crate::Signature25,
    crate::SubspaceId25,
    crate::Signature25,
>;

pub type FailedDelegationError = meadowcap::FailedDelegationError<1024, 1024, 1024, SubspaceId25>;
pub type InvalidDelegationError =
    meadowcap::InvalidDelegationError<1024, 1024, 1024, SubspaceId25, Signature25>;
pub type OwnedCapabilityCreationError = meadowcap::OwnedCapabilityCreationError<NamespaceId25>;

// Straight re-exports for convenience' sake.
pub use meadowcap::AccessMode;
pub use meadowcap::NamespaceIsNotCommunalError;
pub use meadowcap::NotAWriteCapabilityError;
