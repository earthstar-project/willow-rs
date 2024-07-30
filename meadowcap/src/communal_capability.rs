use signature::{Signer, Verifier};
use ufotofu::{local_nb::Consumer, sync::consumer::IntoVec};
use willow_data_model::{
    encoding::{parameters::Encodable, relativity::RelativeEncodable},
    grouping::area::Area,
    parameters::{NamespaceId, SubspaceId},
};

use crate::{AccessMode, Delegation, FailedDelegationError, InvalidDelegationError, IsCommunal};

/// Returned when [`is_communal`](https://willowprotocol.org/specs/meadowcap/index.html#is_communal) unexpectedly mapped a given namespace to `false`.
pub struct NamespaceIsNotCommunalError<NamespacePublicKey>(NamespacePublicKey);

/// A capability that implements [communal namespaces](https://willowprotocol.org/specs/meadowcap/index.html#communal_namespace).
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_capabilities).
pub struct CommunalCapability<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: NamespaceId + Encodable + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    UserSignature: Encodable,
{
    access_mode: AccessMode,
    namespace_key: NamespacePublicKey,
    user_key: UserPublicKey,
    delegations: Vec<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: NamespaceId + Encodable + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    UserSignature: Encodable + Clone,
{
    /// Create a new communal capability granting access to the [`SubspaceId`] corresponding to the given [`UserPublicKey`].
    pub fn new(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, NamespaceIsNotCommunalError<NamespacePublicKey>> {
        if !namespace_key.is_communal() {
            return Err(NamespaceIsNotCommunalError(namespace_key.clone()));
        }

        Ok(Self {
            access_mode,
            namespace_key,
            user_key,
            delegations: Vec::new(),
        })
    }

    /// Delegate this capability to a new [`UserPublicKey`] for a given [`Area`].
    /// Will fail if the area is not included by this capability's [granted area](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_area), or if the given secret key does not correspond to the capability's [receiver](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_receiver).
    pub async fn delegate<UserSecretKey>(
        &self,
        secret_key: UserSecretKey,
        new_user: UserPublicKey,
        new_area: Area<MCL, MCC, MPL, UserPublicKey>,
    ) -> Result<Self, FailedDelegationError<MCL, MCC, MPL, UserPublicKey>>
    where
        UserSecretKey: Signer<UserSignature>,
    {
        let prev_area = self.granted_area();

        if !prev_area.includes_area(&new_area) {
            return Err(FailedDelegationError::AreaNotIncluded {
                excluded_area: new_area,
                claimed_receiver: new_user,
            });
        }

        let prev_user = self.receiver();

        let handover = self.handover(&new_area, &new_user).await;
        let signature = secret_key.sign(&handover);

        prev_user
            .verify(&handover, &signature)
            .map_err(|_| FailedDelegationError::WrongSecretForUser(new_user.clone()))?;

        let mut new_delegations = self.delegations.clone();

        new_delegations.push(Delegation::new(new_area, new_user, signature));

        Ok(Self {
            access_mode: self.access_mode.clone(),
            namespace_key: self.namespace_key.clone(),
            user_key: self.user_key.clone(),
            delegations: new_delegations,
        })
    }

    /// Append an existing delegation to an existing capability, or return an error if the delegation is invalid.
    pub async fn append_existing_delegation(
        &mut self,
        delegation: Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>,
    ) -> Result<(), InvalidDelegationError<MCL, MCC, MPL, UserPublicKey, UserSignature>> {
        let new_area = delegation.area();
        let new_user = delegation.user();
        let new_sig = delegation.signature();

        if !self.granted_area().includes_area(new_area) {
            return Err(InvalidDelegationError::AreaNotIncluded {
                excluded_area: new_area.clone(),
                claimed_receiver: new_user.clone(),
            });
        }

        let handover = self.handover(new_area, new_user).await;

        let prev_receiver = self.receiver();

        prev_receiver.verify(&handover, new_sig).map_err(|_| {
            InvalidDelegationError::InvalidSignature {
                claimed_receiver: new_user.clone(),
                expected_signatory: prev_receiver.clone(),
                signature: new_sig.clone(),
            }
        })?;

        self.delegations.push(delegation);

        Ok(())
    }

    /// The kind of access this capability grants.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_mode)
    pub fn access_mode(&self) -> &AccessMode {
        &self.access_mode
    }

    /// The user to whom this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_receiver)
    pub fn receiver(&self) -> &UserPublicKey {
        if self.delegations.is_empty() {
            return &self.user_key;
        }

        // We can unwrap here because we know delegations isn't empty.
        let last_delegation = self.delegations.last().unwrap();
        let receiver = last_delegation.user();

        receiver
    }

    /// The [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_namespace)
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        &self.namespace_key
    }

    /// The [`Area`] for which this capability grants access.
    ///
    /// [Definition](`https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_area`)
    pub fn granted_area(&self) -> Area<MCL, MCC, MPL, UserPublicKey> {
        if self.delegations.is_empty() {
            return Area::new_subspace(self.user_key.clone());
        }

        // We can unwrap here because we know delegations isn't empty.
        let last_delegation = self.delegations.last().unwrap();

        last_delegation.area().clone()
    }

    /// A bytestring to be signed for a new [`Delegation`].
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_handover)
    // TODO: Make this sync.
    async fn handover(
        &self,
        new_area: &Area<MCL, MCC, MPL, UserPublicKey>,
        new_user: &UserPublicKey,
    ) -> Box<[u8]> {
        let mut consumer = IntoVec::<u8>::new();

        if self.delegations.is_empty() {
            let first_byte = match self.access_mode {
                AccessMode::Read => 0x00,
                AccessMode::Write => 0x01,
            };

            let prev_area =
                Area::<MCL, MCC, MPL, UserPublicKey>::new_subspace(self.user_key.clone());

            // We can safely unwrap all these encodings as IntoVec's error is the never type.
            consumer.consume(first_byte).await.unwrap();
            self.namespace_key.encode(&mut consumer).await.unwrap();
            new_area
                .relative_encode(&prev_area, &mut consumer)
                .await
                .unwrap();
            new_user.encode(&mut consumer).await.unwrap();

            return consumer.into_vec().into();
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        // We can safely unwrap all these encodings as IntoVec's error is the never type.
        new_area
            .relative_encode(prev_area, &mut consumer)
            .await
            .unwrap();
        prev_signature.encode(&mut consumer).await.unwrap();
        new_user.encode(&mut consumer).await.unwrap();

        consumer.into_vec().into()
    }
}
