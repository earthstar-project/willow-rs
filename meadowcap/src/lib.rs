#![doc(html_logo_url = "https://willowprotocol.org/named_assets/meadowcap_emblem_standalone.png")]
//! # Meadowcap
//!
//! An implementation of [Meadowcap](https://willowprotocol.org/specs/meadowcap/index.html#meadowcap), a capability system for permissioning read and write access to the [Willow data model](https://willowprotocol.org/specs/data-model/index.html#data_model).
//!
//! Includes implementations of [communal capabilities](https://willowprotocol.org/specs/meadowcap/index.html#communal_capabilities), [owned capabilities](https://willowprotocol.org/specs/meadowcap/index.html#owned_capabilities), a type [unifying the two](https://willowprotocol.org/specs/meadowcap/index.html#proper_capabilities), as well as the generation of [`McAuthorisationTokens`](https://willowprotocol.org/specs/meadowcap/index.html#MeadowcapAuthorisationToken) for use with the Willow data model's [`is_authorised_write`](https://willowprotocol.org/specs/data-model/index.html#is_authorised_write) parameter.
//!
//! ## Type parameters
//!
//! Willow is a parametrised family of protocols, and so this crate makes heavy use of generic parameters.
//!
//! The following generic parameter names are used consistently across this crate:
//!
//! - `MCL` - A `usize` representing [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_component_length).
//! - `MCC` - A `usize` representing [`max_component_count`](https://willowprotocol.org/specs/data-model/index.html#max_component_count).
//! - `MPL` - A `usize` representing [`max_path_length`](https://willowprotocol.org/specs/data-model/index.html#max_path_length).
//! - `NamespacePublicKey` - The type used for [`NamespacePublicKey`](https://willowprotocol.org/specs/meadowcap/index.html#NamespacePublicKey) (willowprotocol.org), must implement the [`willow_data_model::NamespaceId`] trait.
//! - `NamespaceSignature` - The type used for [`NamespaceSignature`](https://willowprotocol.org/specs/meadowcap/index.html#NamespaceSignature) (willowprotocol.org).
//! - `UserPublicKey` - The type used for [`UserPublicKey`](https://willowprotocol.org/specs/meadowcap/index.html#UserPublicKey) (willowprotocol.org), must implement the [`SubspaceId`] trait.
//! - `UserSignature` - The type used for [`UserSignature`](https://willowprotocol.org/specs/meadowcap/index.html#UserSignature) (willowprotocol.org).
//! - `PD` - The type used for [`PayloadDigest`](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest) (willowprotocol.org), must implement the [`willow_data_model::PayloadDigest`] trait.

use willow_data_model::{grouping::Area, SubspaceId};

/// Maps [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) public keys to booleans, determining whether that namespace of a particular [`willow_data_model::NamespaceId`] is [communal](https://willowprotocol.org/specs/meadowcap/index.html#communal_namespace) or [owned](https://willowprotocol.org/specs/meadowcap/index.html#owned_namespace).
pub trait IsCommunal {
    fn is_communal(&self) -> bool;
}

/// A delegation of access rights to a user for a given [area](https://willowprotocol.org/specs/grouping-entries/index.html#areas).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Delegation<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    UserPublicKey,
    UserSignature,
> {
    area: Area<MCL, MCC, MPL, UserPublicKey>,
    user: UserPublicKey,
    signature: UserSignature,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey, UserSignature>
    Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>
where
    UserPublicKey: McPublicUserKey<UserSignature>,
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
mod silly_sigs;
#[cfg(feature = "dev")]
pub use silly_sigs::*;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey, UserSignature>
    Arbitrary<'a> for Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>
where
    UserSignature: Arbitrary<'a>,
    UserPublicKey: McPublicUserKey<UserSignature> + Arbitrary<'a>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Returned when an attempt to delegate a capability to another `UserPublicKey` failed.
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
    /// The given secret did not correspond to the receiver of the existing capability we tried to delegate from.
    WrongSecretForUser(UserPublicKey),
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey: SubspaceId>
    core::fmt::Display for FailedDelegationError<MCL, MCC, MPL, UserPublicKey>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailedDelegationError::AreaNotIncluded {
                excluded_area: _,
                claimed_receiver: _,
            } => write!(
                f,
                "Tried to delegate access to an area not fully included by the granted area of the capability being delegated from."
            ),
            FailedDelegationError::WrongSecretForUser(_) => write!(
                f,
                "Provided the wrong secret for the existing capability's receiver."
            ),
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, UserPublicKey: SubspaceId>
    std::error::Error for FailedDelegationError<MCL, MCC, MPL, UserPublicKey>
{
}

/// Returned when an existing delegation was an invalid successor to an existing delegation chain.
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

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        UserPublicKey: McPublicUserKey<UserSignature>,
        UserSignature,
    > core::fmt::Display for InvalidDelegationError<MCL, MCC, MPL, UserPublicKey, UserSignature>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidDelegationError::AreaNotIncluded {
                excluded_area: _,
                claimed_receiver: _,
            } => write!(
                f,
                "Tried to append a delegation which grants access to an area not fully included by the granted area of the capability."
            ),
            InvalidDelegationError::InvalidSignature {
                expected_signatory: _,
                claimed_receiver: _,
                signature: _,
            } => write!(
                f,
                "Tried to append a delegation with an invalid signature for the receiver of the target capability."
            ),
        }
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        UserPublicKey: McPublicUserKey<UserSignature>,
        UserSignature: std::fmt::Debug,
    > core::error::Error for InvalidDelegationError<MCL, MCC, MPL, UserPublicKey, UserSignature>
{
}

mod communal_capability;
pub use communal_capability::*;

mod mc_authorisation_token;
pub use mc_authorisation_token::*;

mod mc_capability;
pub use mc_capability::*;

mod owned_capability;
pub use owned_capability::*;

mod mc_subspace_capability;
pub use mc_subspace_capability::*;

mod parameters;
pub use parameters::*;

mod personal_private_interest;
pub use personal_private_interest::*;
