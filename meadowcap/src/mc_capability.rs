use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu::sync::consumer::IntoVec;
use willow_data_model::{
    encoding::parameters_sync::Encodable,
    entry::Entry,
    grouping::area::Area,
    parameters::{NamespaceId, PayloadDigest, SubspaceId},
};

use crate::{
    communal_capability::{CommunalCapability, NamespaceIsNotCommunalError},
    mc_authorisation_token::McAuthorisationToken,
    owned_capability::{OwnedCapability, OwnedCapabilityCreationError},
    AccessMode, FailedDelegationError, IsCommunal,
};

/// A Meadowcap capability.
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#Capability)
#[derive(Clone)]
pub enum McCapability<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    NamespaceSignature: Encodable + Clone,
    UserSignature: Encodable + Clone,
{
    Communal(CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>),
    Owned(
        OwnedCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
    ),
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
    McCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    NamespaceSignature: Encodable + Clone,
    UserSignature: Encodable + Clone,
{
    /// Create a new communal capability granting access to the [`SubspaceId`] corresponding to the given [`UserPublicKey`], or return an error if the namespace key is not communal.
    pub fn new_communal(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, NamespaceIsNotCommunalError<NamespacePublicKey>> {
        let cap = CommunalCapability::new(namespace_key, user_key, access_mode)?;
        Ok(Self::Communal(cap))
    }

    /// Create a new owned capability granting access to the [full area](https://willowprotocol.org/specs/grouping-entries/index.html#full_area) of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) to the given [`UserPublicKey`].
    pub async fn new_owned<NamespaceSecret>(
        namespace_key: NamespacePublicKey,
        namespace_secret: NamespaceSecret,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, OwnedCapabilityCreationError<NamespacePublicKey>>
    where
        NamespaceSecret: Signer<NamespaceSignature>,
    {
        let cap =
            OwnedCapability::new(namespace_key, namespace_secret, user_key, access_mode).await?;

        Ok(Self::Owned(cap))
    }

    /// The kind of access this capability grants.
    pub fn access_mode(&self) -> &AccessMode {
        match self {
            Self::Communal(cap) => cap.access_mode(),
            Self::Owned(cap) => cap.access_mode(),
        }
    }

    /// The user to whom this capability grants access.
    pub fn receiver(&self) -> &UserPublicKey {
        match self {
            Self::Communal(cap) => cap.receiver(),
            Self::Owned(cap) => cap.receiver(),
        }
    }

    /// The [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        match self {
            Self::Communal(cap) => cap.granted_namespace(),
            Self::Owned(cap) => cap.granted_namespace(),
        }
    }

    /// The [`Area`] for which this capability grants access.
    pub fn granted_area(&self) -> Area<MCL, MCC, MPL, UserPublicKey> {
        match self {
            Self::Communal(cap) => cap.granted_area(),
            Self::Owned(cap) => cap.granted_area(),
        }
    }

    /// Delegate this capability to a new [`UserPublicKey`] for a given [`Area`].
    /// Will fail if the area is not included by this capability's granted area, or if the given secret key does not correspond to the capability's receiver.
    pub fn delegate<UserSecretKey>(
        &self,
        secret_key: UserSecretKey,
        new_user: UserPublicKey,
        new_area: Area<MCL, MCC, MPL, UserPublicKey>,
    ) -> Result<Self, FailedDelegationError<MCL, MCC, MPL, UserPublicKey>>
    where
        UserSecretKey: Signer<UserSignature>,
    {
        let delegated = match self {
            McCapability::Communal(cap) => {
                let delegated = cap.delegate(secret_key, new_user, new_area)?;
                Self::Communal(delegated)
            }
            McCapability::Owned(cap) => {
                let delegated = cap.delegate(secret_key, new_user, new_area)?;
                Self::Owned(delegated)
            }
        };

        Ok(delegated)
    }

    /// Return a new AuthorisationToken without checking if the resulting signature is correct (e.g. because you are going to immediately do that by constructing an [`willow_data_model::AuthorisedEntry`]).
    pub fn authorisation_token<UserSecret, PD>(
        &self,
        entry: Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        secret: UserSecret,
    ) -> McAuthorisationToken<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
    where
        UserSecret: Signer<UserSignature>,
        PD: PayloadDigest + Encodable,
    {
        let mut consumer = IntoVec::<u8>::new();
        entry.encode(&mut consumer).unwrap();

        let signature = secret.sign(&consumer.into_vec());

        McAuthorisationToken {
            capability: self.clone(),
            signature,
        }
    }

    /// Return a new [`AuthorisationToken`], or an error if the resulting signature was not for the capability's receiver.
    pub fn authorisation_token_checked<UserSecret, PD>(
        &self,
        entry: Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        secret: UserSecret,
    ) -> Result<
        McAuthorisationToken<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        SignatureError,
    >
    where
        UserSecret: Signer<UserSignature>,
        PD: PayloadDigest + Encodable,
    {
        let mut consumer = IntoVec::<u8>::new();
        entry.encode(&mut consumer).unwrap();

        let message = consumer.into_vec();

        let signature = secret.sign(&message);

        self.receiver().verify(&message, &signature)?;

        Ok(McAuthorisationToken {
            capability: self.clone(),
            signature,
        })
    }
}
