use signature::Signer;
use ufotofu_codec::{
    Encodable, EncodableKnownSize, EncodableSync, RelativeEncodable, RelativeEncodableKnownSize,
};
use willow_data_model::grouping::Area;

use crate::{
    AccessMode, Delegation, FailedDelegationError, InvalidDelegationError, McNamespaceId,
    McSubspaceId,
};

/// Returned when [`is_communal`](https://willowprotocol.org/specs/meadowcap/index.html#is_communal) unexpectedly mapped a given [`namespace`](https://willowprotocol.org/specs/data-model/index.html#namespace) to `false`.
#[derive(Debug)]
pub struct NamespaceIsNotCommunalError<NamespacePublicKey>(NamespacePublicKey);

impl<NamespacePublicKey> core::fmt::Display for NamespaceIsNotCommunalError<NamespacePublicKey> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tried to construct a communal capability for an owned namespace."
        )
    }
}

impl<NamespacePublicKey: std::fmt::Debug> std::error::Error
    for NamespaceIsNotCommunalError<NamespacePublicKey>
{
}

/// A capability which implements [communal namespaces](https://willowprotocol.org/specs/meadowcap/index.html#communal_namespace).
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_capabilities).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommunalCapability<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
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
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    /// Creates a new communal capability granting access to the [`SubspaceId`] corresponding to the given `UserPublicKey`.
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

    /// Delegates this capability to a new `UserPublicKey` for a given [`willow_data_model::grouping::Area`].
    /// Will fail if the area is not included by this capability's [granted area](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_area), or if the given secret key does not correspond to the capability's [receiver](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_receiver).
    pub fn delegate<UserSecretKey>(
        &self,
        secret_key: &UserSecretKey,
        new_user: &UserPublicKey,
        new_area: &Area<MCL, MCC, MPL, UserPublicKey>,
    ) -> Result<Self, FailedDelegationError<MCL, MCC, MPL, UserPublicKey>>
    where
        UserSecretKey: Signer<UserSignature>,
    {
        let prev_area = self.granted_area();

        if !prev_area.includes_area(new_area) {
            return Err(FailedDelegationError::AreaNotIncluded {
                excluded_area: new_area.clone(),
                claimed_receiver: new_user.clone(),
            });
        }

        let prev_user = self.receiver();

        let handover = CommunalHandover {
            cap: self,
            area: new_area,
            user: new_user,
        };

        let handover_enc = &handover.sync_encode_into_boxed_slice();

        let signature = secret_key.sign(&handover_enc);

        prev_user
            .verify(&handover_enc, &signature)
            .map_err(|_| FailedDelegationError::WrongSecretForUser(self.receiver().clone()))?;

        let mut new_delegations = self.delegations.clone();

        new_delegations.push(Delegation::new(
            new_area.clone(),
            new_user.clone(),
            signature,
        ));

        Ok(Self {
            access_mode: self.access_mode,
            namespace_key: self.namespace_key.clone(),
            user_key: self.user_key.clone(),
            delegations: new_delegations,
        })
    }

    /// Appends an existing delegation to an existing capability, or return an error if the delegation is invalid.
    pub fn append_existing_delegation(
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

        let handover = CommunalHandover {
            cap: self,
            area: new_area,
            user: new_user,
        };

        let prev_receiver = self.receiver();

        let handover_slice = handover.sync_encode_into_boxed_slice();

        prev_receiver
            .verify(&handover_slice, new_sig)
            .map_err(|_| InvalidDelegationError::InvalidSignature {
                claimed_receiver: new_user.clone(),
                expected_signatory: prev_receiver.clone(),
                signature: new_sig.clone(),
            })?;

        self.delegations.push(delegation);

        Ok(())
    }

    /// Returns the kind of access this capability grants.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_mode)
    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    /// Returns the public key of the user to whom this capability grants access.
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

    /// Returns the public key of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_granted_namespace)
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        &self.namespace_key
    }

    /// Returns the [`Area`] for which this capability grants access.
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

    /// Returns a slice of all [`Delegation`]s made to this capability, with a concrete return type.
    pub(crate) fn delegations_(
        &self,
    ) -> core::slice::Iter<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>> {
        self.delegations.iter()
    }

    /// Returns a slice of all [`Delegation`]s made to this capability.
    pub fn delegations(
        &self,
    ) -> impl ExactSizeIterator<Item = &Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>
    {
        self.delegations_()
    }

    /// Returns the number of delegations present on this capability.
    pub fn delegations_len(&self) -> usize {
        self.delegations.len()
    }

    /// Returns the public key of the very first user this capability was issued to.
    pub fn progenitor(&self) -> &UserPublicKey {
        &self.user_key
    }
}

/// Can be encoded to a bytestring to be signed for a new [`Delegation`] to a [`CommunalCapability`].
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_handover)
pub struct CommunalHandover<
    'a,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    cap: &'a CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>,
    area: &'a Area<MCL, MCC, MPL, UserPublicKey>,
    user: &'a UserPublicKey,
}

impl<
        'a,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > Encodable
    for CommunalHandover<'a, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if self.cap.delegations.is_empty() {
            let first_byte = match self.cap.access_mode {
                AccessMode::Read => 0x00,
                AccessMode::Write => 0x01,
            };

            let prev_area =
                Area::<MCL, MCC, MPL, UserPublicKey>::new_subspace(self.cap.user_key.clone());

            // We can safely unwrap all these encodings as IntoVec's error is the never type.
            consumer.consume(first_byte).await?;
            self.cap.namespace_key.encode(consumer).await?;
            self.area.relative_encode(consumer, &prev_area).await?;
            self.user.encode(consumer).await?;

            return Ok(());
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.cap.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        // We can safely unwrap all these encodings as IntoVec's error is the never type.
        self.area.relative_encode(consumer, prev_area).await?;
        prev_signature.encode(consumer).await?;
        self.user.encode(consumer).await?;

        Ok(())
    }
}

impl<
        'a,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > EncodableKnownSize
    for CommunalHandover<'a, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        if self.cap.delegations.is_empty() {
            let prev_area =
                Area::<MCL, MCC, MPL, UserPublicKey>::new_subspace(self.cap.user_key.clone());

            let namespace_len = self.cap.namespace_key.len_of_encoding();
            let area_len = self.area.relative_len_of_encoding(&prev_area);
            let user_len = self.user.len_of_encoding();

            return 1 + namespace_len + area_len + user_len;
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.cap.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        let area_len = self.area.relative_len_of_encoding(prev_area);
        let sig_len = prev_signature.len_of_encoding();
        let user_len = self.user.len_of_encoding();

        area_len + sig_len + user_len
    }
}

impl<
        'a,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > EncodableSync
    for CommunalHandover<'a, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespaceId,
    UserPublicKey: McSubspaceId<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
}

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

#[cfg(feature = "dev")]
impl<
        'a,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > Arbitrary<'a>
    for CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespaceId + Arbitrary<'a>,
    UserPublicKey: McSubspaceId<UserSignature> + Arbitrary<'a>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_key: NamespacePublicKey = Arbitrary::arbitrary(u)?;
        let user_key: UserPublicKey = Arbitrary::arbitrary(u)?;
        let access_mode: AccessMode = Arbitrary::arbitrary(u)?;

        match Self::new(namespace_key, user_key, access_mode) {
            Ok(cap) => Ok(cap),
            Err(_) => Err(ArbitraryError::IncorrectFormat),
        }
    }
}
