use ufotofu::{local_nb::Consumer, sync::consumer::IntoVec};
use willow_data_model::{
    encoding::{parameters::Encodable, relativity::RelativeEncodable},
    grouping::area::Area,
    parameters::{NamespaceId, SubspaceId},
};

/// Can be used to sign a bytestring.
pub trait Signing<PublicKey, Signature> {
    fn corresponding_public_key(&self) -> PublicKey;
    fn sign(&self, bytestring: &[u8]) -> Signature;
}

/// Indicates that this is a verifiable signature.
pub trait Verifiable<PublicKey> {
    fn verify(&self, public_key: &PublicKey, bytestring: &[u8]) -> bool;
}

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
    AreaNotIncluded(Area<MCL, MCC, MPL, UserPublicKey>, UserPublicKey),
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
    AreaNotIncluded((Area<MCL, MCC, MPL, UserPublicKey>, UserPublicKey)),
    /// The signature of the delegation was not valid for the recevier of the capability we tried to add the delegation to.
    InvalidSignature((UserPublicKey, UserSignature)),
}

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
    NamespacePublicKey: NamespaceId + Encodable,
    UserPublicKey: SubspaceId + Encodable,
    UserSignature: Encodable + Verifiable<UserPublicKey>,
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
    NamespacePublicKey: NamespaceId + Encodable,
    UserPublicKey: SubspaceId + Encodable,
    UserSignature: Encodable + Verifiable<UserPublicKey> + Clone,
{
    /// Create a new communal capability granting access to the [`SubspaceId`] corresponding to the given [`UserPublicKey`].
    pub fn new(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Self {
        Self {
            access_mode,
            namespace_key,
            user_key,
            delegations: Vec::new(),
        }
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
        UserSecretKey: Signing<UserPublicKey, UserSignature>,
    {
        let prev_area = self.granted_area();

        if !prev_area.includes_area(&new_area) {
            return Err(FailedDelegationError::AreaNotIncluded(new_area, new_user));
        }

        let prev_user = self.receiver();

        if &secret_key.corresponding_public_key() != prev_user {
            return Err(FailedDelegationError::WrongSecretForUser(new_user));
        }

        let handover = self.handover(&new_area, &new_user).await;
        let signature = secret_key.sign(&handover);

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

        if !self.granted_area().includes_area(&new_area) {
            return Err(InvalidDelegationError::AreaNotIncluded((
                new_area.clone(),
                new_user.clone(),
            )));
        }

        let handover = self.handover(&new_area, &new_user).await;

        let prev_receiver = self.receiver();

        let is_authentic = new_sig.verify(prev_receiver, &handover);

        if !is_authentic {
            return Err(InvalidDelegationError::InvalidSignature((
                new_user.clone(),
                new_sig.clone(),
            )));
        }

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
    /// [Definition](`https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_namespace`)
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
    ) -> Vec<u8> {
        let mut consumer = IntoVec::<u8>::new();

        if self.delegations.is_empty() {
            let first_byte = match self.access_mode {
                AccessMode::Read => 0x00,
                AccessMode::Write => 0x01,
            };

            let prev_area =
                Area::<MCL, MCC, MPL, UserPublicKey>::new_subspace(self.user_key.clone());

            // TODO: Decide whether to propagate these errors or not.
            consumer.consume(first_byte).await;
            self.namespace_key.encode(&mut consumer).await;
            new_area.relative_encode(&prev_area, &mut consumer).await;
            new_user.encode(&mut consumer).await;

            return consumer.into_vec();
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        new_area.relative_encode(prev_area, &mut consumer).await;
        prev_signature.encode(&mut consumer).await;
        new_user.encode(&mut consumer).await;

        consumer.into_vec()
    }
}
