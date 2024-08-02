use willow_data_model::{grouping::area::Area, parameters::SubspaceId};

/// Maps namespace public keys to booleans, determining whether that namespace of a particular [`willow_data_model::NamespaceId`] is [communal](https://willowprotocol.org/specs/meadowcap/index.html#communal_namespace) or [owned](https://willowprotocol.org/specs/meadowcap/index.html#owned_namespace).
pub trait IsCommunal {
    fn is_communal(&self) -> bool;
}

/// A delegation of access rights to a user for a given area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delegation<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    UserPublicKey,
    UserSignature,
> where
    UserPublicKey: SubspaceId,
{
    area: Area<MCL, MCC, MPL, UserPublicKey>,
    user: UserPublicKey,
    signature: UserSignature,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey, UserSignature>
    Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>
where
    UserPublicKey: SubspaceId,
{
    pub fn new(
        area: Area<MCL, MCC, MPL, UserPublicKey>,
        user: UserPublicKey,
        signature: UserSignature,
    ) -> Self {
        Self {
            area,
            user,
            signature,
        }
    }

    /// The granted area of this delegation.
    pub fn area(&self) -> &Area<MCL, MCC, MPL, UserPublicKey> {
        &self.area
    }

    /// The user delegated to.
    pub fn user(&self) -> &UserPublicKey {
        &self.user
    }

    /// The signature of the user who created this delegation.
    pub fn signature(&self) -> &UserSignature {
        &self.signature
    }
}

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey, UserSignature>
    Arbitrary<'a> for Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>
where
    UserSignature: Arbitrary<'a>,
    UserPublicKey: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let area: Area<MCL, MCC, MPL, UserPublicKey> = Arbitrary::arbitrary(u)?;
        let user: UserPublicKey = Arbitrary::arbitrary(u)?;
        let signature: UserSignature = Arbitrary::arbitrary(u)?;

        Ok(Self {
            area,
            signature,
            user,
        })
    }
}

/// A mode granting read or write access to some [`Area`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    Write,
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for AccessMode {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let read: bool = Arbitrary::arbitrary(u)?;

        if read {
            Ok(Self::Read)
        } else {
            Ok(Self::Write)
        }
    }
}

/// Returned when an attempt to delegate a capability failed.
#[derive(Debug)]
pub enum FailedDelegationError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    UserPublicKey: SubspaceId,
> {
    /// The granted area of the capability we tried to delegate from did not include the given area.
    AreaNotIncluded {
        excluded_area: Area<MCL, MCC, MPL, UserPublicKey>,
        claimed_receiver: UserPublicKey,
    },
    /// The given secret did not correspond to the receiver of the capability we tried to delegate from.
    WrongSecretForUser(UserPublicKey),
}

/// Returned when an existing delegation was found to be invalid.
#[derive(Debug)]
pub enum InvalidDelegationError<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    UserPublicKey: SubspaceId,
    UserSignature,
> {
    /// The delegation's area was not included by the granted area of the capability we tried to add it to.
    AreaNotIncluded {
        excluded_area: Area<MCL, MCC, MPL, UserPublicKey>,
        claimed_receiver: UserPublicKey,
    },
    /// The signature of the delegation was not valid for the receiver of the capability we tried to add the delegation to.
    InvalidSignature {
        expected_signatory: UserPublicKey,
        claimed_receiver: UserPublicKey,
        signature: UserSignature,
    },
}

pub mod communal_capability;
pub mod mc_authorisation_token;
pub mod mc_capability;
pub mod owned_capability;
