use willow_data_model::{grouping::area::Area, parameters::SubspaceId};

/// A delegation of access rights to a user for a given area.
#[derive(Clone)]
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

/// A mode granting read or write access to some [`Area`].
#[derive(Clone)]
pub enum AccessMode {
    Read,
    Write,
}

/// Returned when an attempt to delegate a capability failed.
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
pub mod owned_capability;
