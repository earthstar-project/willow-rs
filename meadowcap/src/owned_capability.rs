use compact_u64::{CompactU64, Tag, TagWidth};
use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable, RelativeEncodableKnownSize,
};
#[cfg(feature = "dev")]
use willow_data_model::grouping::arbitrary_included_area;
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    Entry, Path, SubspaceId,
};
use willow_encoding::is_bitflagged;
use willow_pio::{PrivateAreaContext, PrivateInterest};

use crate::{
    AccessMode, Delegation, FailedDelegationError, InvalidDelegationError, McCapability,
    McNamespacePublicKey, McPublicUserKey,
};

#[cfg(feature = "dev")]
use crate::{SillyPublicKey, SillySig};

/// Returned when an attempt to create a new owned capability failed.
#[derive(Debug)]
pub enum OwnedCapabilityCreationError<NamespacePublicKey> {
    /// [`is_communal`](https://willowprotocol.org/specs/meadowcap/index.html#is_communal) unexpectedly mapped a given namespace to `true`.
    NamespaceIsCommunal(NamespacePublicKey),
    /// The resulting signature was faulty, probably due to the wrong secret being given.
    InvalidSignature(SignatureError),
}

impl<NamespacePublicKey> core::fmt::Display for OwnedCapabilityCreationError<NamespacePublicKey> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwnedCapabilityCreationError::NamespaceIsCommunal(_) => write!(
                f,
                "Tried to construct an owned capability for a communal namespace."
            ),
            OwnedCapabilityCreationError::InvalidSignature(_) => write!(
                f,
                "Tried to construct an owned capability with an invalid initial authorisation signature."
            ),
        }
    }
}

impl<NamespacePublicKey: std::fmt::Debug> std::error::Error
    for OwnedCapabilityCreationError<NamespacePublicKey>
{
}

/// A capability that implements [owned namespaces](https://willowprotocol.org/specs/meadowcap/index.html#owned_namespace).
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#owned_capabilities).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OwnedCapability<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> {
    access_mode: AccessMode,
    namespace_key: NamespacePublicKey,
    user_key: UserPublicKey,
    initial_authorisation: NamespaceSignature,
    delegations: Vec<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>,
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
    OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq,
{
    /// Creates a new owned capability granting access to the [full area](https://willowprotocol.org/specs/grouping-entries/index.html#full_area) of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) to the given `UserPublicKey`.
    pub fn new<NamespaceSecret>(
        namespace_key: NamespacePublicKey,
        namespace_secret: &NamespaceSecret,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, OwnedCapabilityCreationError<NamespacePublicKey>>
    where
        NamespaceSecret: Signer<NamespaceSignature>,
    {
        if namespace_key.is_communal() {
            return Err(OwnedCapabilityCreationError::NamespaceIsCommunal(
                namespace_key.clone(),
            ));
        }

        let message = OwnedInitialAuthorisationMsg {
            access_mode,
            user_key: &user_key,
        };

        let message_enc = message.sync_encode_into_boxed_slice();

        let initial_authorisation = namespace_secret.sign(&message_enc);

        namespace_key
            .verify(&message_enc, &initial_authorisation)
            .map_err(|err| OwnedCapabilityCreationError::InvalidSignature(err))?;

        Ok(Self {
            access_mode,
            namespace_key,
            initial_authorisation,
            user_key,
            delegations: Vec::new(),
        })
    }

    /// Creates an [`OwnedCapability`] using an existing authorisation (e.g. one received over the network), or return an error if the signature was not created by the namespace key.
    pub fn from_existing(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        initial_authorisation: NamespaceSignature,
        access_mode: AccessMode,
    ) -> Result<Self, OwnedCapabilityCreationError<NamespacePublicKey>> {
        let message = OwnedInitialAuthorisationMsg {
            access_mode,
            user_key: &user_key,
        };

        let message_enc = message.sync_encode_into_boxed_slice();

        namespace_key
            .verify(&message_enc, &initial_authorisation)
            .map_err(|err| OwnedCapabilityCreationError::InvalidSignature(err))?;

        Ok(Self {
            access_mode,
            namespace_key,
            user_key,
            initial_authorisation,
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

        let handover = OwnedHandover {
            cap: self,
            area: new_area,
            user: new_user,
        };

        let handover_enc = handover.sync_encode_into_boxed_slice();

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
            initial_authorisation: self.initial_authorisation.clone(),
            user_key: self.user_key.clone(),
            delegations: new_delegations,
        })
    }

    /// Returns whether this capability needs a complementing [`crate::McSubspaceCapability`] [(definition)](https://willowprotocol.org/specs/pai/index.html#subspace_capability) to in order to be fully authorised by the Willow General Sync Protocol.
    pub fn needs_subspace_cap(&self) -> bool {
        if self.access_mode == AccessMode::Write {
            return false;
        }

        let granted_area = self.granted_area();

        if granted_area.subspace() != &AreaSubspace::Any {
            return false;
        }

        if granted_area.path() == &Path::new_empty() {
            return false;
        }

        true
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

        let handover = OwnedHandover {
            cap: self,
            area: new_area,
            user: new_user,
        };

        let handover_enc = handover.sync_encode_into_boxed_slice();

        let prev_receiver = self.receiver();

        prev_receiver.verify(&handover_enc, new_sig).map_err(|_| {
            InvalidDelegationError::InvalidSignature {
                claimed_receiver: new_user.clone(),
                expected_signatory: prev_receiver.clone(),
                signature: new_sig.clone(),
            }
        })?;

        self.delegations.push(delegation);

        Ok(())
    }

    /// Returns the kind of access this capability grants.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#owned_cap_mode)
    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    /// Returns the public key of the user to whom this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#owned_cap_receiver)
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
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#owned_cap_granted_namespace)
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        &self.namespace_key
    }

    /// Returns [`Area`] for which this capability grants access.
    ///
    /// [Definition](`https://willowprotocol.org/specs/meadowcap/index.html#owned_cap_granted_area`)
    pub fn granted_area(&self) -> Area<MCL, MCC, MPL, UserPublicKey> {
        if self.delegations.is_empty() {
            return Area::new_full();
        }

        // We can unwrap here because we know delegations isn't empty.
        let last_delegation = self.delegations.last().unwrap();

        last_delegation.area().clone()
    }

    /// Returns a slice of all [`Delegation`]s made to this capability, with a concrete return type.
    pub(crate) fn delegations_(
        &'_ self,
    ) -> core::slice::Iter<'_, Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>> {
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

    /// Returns the original signature authorising this namespace capability.
    pub fn initial_authorisation(&self) -> &NamespaceSignature {
        &self.initial_authorisation
    }

    /// Returns a [`OwnedCapability`] without checking if its initial authorisation or delegations are valid.
    ///
    /// # Safety
    /// Calling this method with an invalid initial authorisation or delegation is immediate undefined behaviour!
    pub unsafe fn new_unchecked(
        access_mode: AccessMode,
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        initial_authorisation: NamespaceSignature,
        delegations: Vec<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>,
    ) -> Self {
        Self {
            access_mode,
            namespace_key,
            user_key,
            initial_authorisation,
            delegations,
        }
    }

    /// Returns the length of the longest common prefix of this capability's delegation chain and the given delegation chain.
    pub fn longest_common_delegation_prefix_len(
        &self,
        delegations: &[Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>],
    ) -> usize {
        let mut count = 0;

        while count < std::cmp::min(self.delegations.len(), delegations.len()) {
            if self.delegations[count] == delegations[count] {
                count += 1;
            } else {
                break;
            }
        }

        count
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
        PD,
    >
    RelativeEncodable<(
        McCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
    )>
    for OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(
            McCapability<
                MCL,
                MCC,
                MPL,
                NamespacePublicKey,
                NamespaceSignature,
                UserPublicKey,
                UserSignature,
            >,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let (prior_cap, entry) = r;

        if let AccessMode::Read = prior_cap.access_mode() {
            panic!("Tried to encode a read capability relative to a write capability")
        }

        if let AccessMode::Read = self.access_mode {
            panic!("Tried to encode a read capability relative to an entry")
        }

        if &self.namespace_key != entry.namespace_id() || !self.granted_area().includes_entry(entry)
        {
            panic!("Tried to encode a communal capability relative to an entry it cannot authorise")
        }

        let shared = match &prior_cap {
            McCapability::Communal(_communal_capability) => 0,
            McCapability::Owned(owned_capability) => {
                self.longest_common_delegation_prefix_len(&owned_capability.delegations)
            }
        };

        let nice_hack = match &prior_cap {
            McCapability::Communal(_communal_capability) => 0,
            McCapability::Owned(owned_capability) => {
                if self.user_key != owned_capability.user_key
                    || self.initial_authorisation != owned_capability.initial_authorisation
                {
                    0
                } else {
                    shared + 1
                }
            }
        };

        let nice_hack_tag = Tag::min_tag(nice_hack as u64, TagWidth::three());
        let del_len_tag = Tag::min_tag(self.delegations_len() as u64, TagWidth::four());

        let mut header = 0b1000_0000;

        header |= nice_hack_tag.data_at_offset(1);
        header |= del_len_tag.data_at_offset(4);

        consumer.consume(header).await?;

        if nice_hack == 0 {
            self.user_key.encode(consumer).await?;
            self.initial_authorisation.encode(consumer).await?;
        }

        let mut prev_area = Area::new_subspace(entry.subspace_id().clone());

        for (i, del) in self.delegations().enumerate() {
            if i < shared {
                continue;
            }

            let private_interest = PrivateInterest::new(
                entry.namespace_id().clone(),
                willow_data_model::grouping::AreaSubspace::Id(entry.subspace_id().clone()),
                entry.path().clone(),
            );

            let ctx = if i == 0 {
                PrivateAreaContext::new(
                    private_interest,
                    Area::new_subspace(entry.subspace_id().clone()),
                )
                .unwrap()
            } else {
                PrivateAreaContext::new(private_interest, prev_area).unwrap()
            };

            prev_area = del.area().clone();

            del.area().relative_encode(consumer, &ctx).await?;
            del.user().encode(consumer).await?;
            del.signature().encode(consumer).await?;
        }

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
        PD,
    >
    RelativeDecodable<
        (
            McCapability<
                MCL,
                MCC,
                MPL,
                NamespacePublicKey,
                NamespaceSignature,
                UserPublicKey,
                UserSignature,
            >,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
        Blame,
    >
    for OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq + Decodable,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq + Decodable,
    Blame: From<UserPublicKey::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserSignature::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(
            McCapability<
                MCL,
                MCC,
                MPL,
                NamespacePublicKey,
                NamespaceSignature,
                UserPublicKey,
                UserSignature,
            >,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        if !is_bitflagged(header, 0) {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        let nice_hack_tag = Tag::from_raw(header, TagWidth::three(), 1);
        let del_len_tag = Tag::from_raw(header, TagWidth::four(), 4);

        let nice_hack = CompactU64::relative_decode(producer, &nice_hack_tag)
            .await
            .map_err(DecodeError::map_other_from)?;
        let del_len = CompactU64::relative_decode(producer, &del_len_tag)
            .await
            .map_err(DecodeError::map_other_from)?;

        let (prior_cap, entry) = r;

        let user_key = if nice_hack.0 == 0 {
            UserPublicKey::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)
        } else {
            match prior_cap {
                McCapability::Communal(_communal_capability) => {
                    Err(DecodeError::Other(Blame::OurFault))
                }
                McCapability::Owned(owned_capability) => Ok(owned_capability.user_key.clone()),
            }
        }?;

        let initial_authorisation = if nice_hack.0 == 0 {
            NamespaceSignature::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)
                .map(Some)?
        } else {
            None
        };

        let init_auth_to_use = match prior_cap {
            McCapability::Communal(_communal_capability) => {
                if let Some(init_auth) = initial_authorisation {
                    Ok(init_auth)
                } else {
                    Err(DecodeError::Other(Blame::OurFault))
                }
            }
            McCapability::Owned(owned_capability) => {
                if let Some(init_auth) = initial_authorisation {
                    Ok(init_auth)
                } else {
                    Ok(owned_capability.initial_authorisation.clone())
                }
            }
        }?;

        let mut decoded_cap = OwnedCapability::<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >::from_existing(
            entry.namespace_id().clone(),
            user_key,
            init_auth_to_use,
            AccessMode::Write,
        )
        .map_err(|_err| DecodeError::Other(Blame::OurFault))?;

        if del_len.0 > 0 {
            let how_many_shared = if nice_hack.0 == 0 { 0 } else { nice_hack.0 - 1 };

            for (i, del) in prior_cap.delegations().enumerate() {
                if (i as u64) < how_many_shared {
                    decoded_cap
                        .append_existing_delegation(del.clone())
                        .map_err(|_err| DecodeError::Other(Blame::OurFault))?;
                }
            }

            let how_many_to_decode = if nice_hack.0 == 0 {
                del_len.0
            } else {
                del_len
                    .0
                    .checked_sub(nice_hack.0 - 1)
                    .ok_or(DecodeError::Other(Blame::TheirFault))?
            };

            let mut prev_area = Area::new_subspace(entry.subspace_id().clone());

            for _i in 0..how_many_to_decode {
                let private_interest = PrivateInterest::new(
                    entry.namespace_id().clone(),
                    willow_data_model::grouping::AreaSubspace::Id(entry.subspace_id().clone()),
                    entry.path().clone(),
                );

                let ctx_to_use = PrivateAreaContext::new(private_interest, prev_area).unwrap();

                let area = Area::relative_decode(producer, &ctx_to_use)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                prev_area = area.clone();

                let user_key = UserPublicKey::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let delegation = Delegation::new(area, user_key, sig);

                decoded_cap
                    .append_existing_delegation(delegation)
                    .map_err(|_err| DecodeError::Other(Blame::TheirFault))?;
            }
        };

        Ok(decoded_cap)
    }
}

struct OwnedInitialAuthorisationMsg<'a, UserPublicKey>
where
    UserPublicKey: SubspaceId + Encodable,
{
    access_mode: AccessMode,
    user_key: &'a UserPublicKey,
}

impl<UserPublicKey> Encodable for OwnedInitialAuthorisationMsg<'_, UserPublicKey>
where
    UserPublicKey: SubspaceId + Encodable,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let access_byte = match self.access_mode {
            AccessMode::Read => 0x02,
            AccessMode::Write => 0x03,
        };

        consumer.consume(access_byte).await?;
        self.user_key.encode(consumer).await?;

        Ok(())
    }
}

impl<UserPublicKey> EncodableKnownSize for OwnedInitialAuthorisationMsg<'_, UserPublicKey>
where
    UserPublicKey: SubspaceId + EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        1 + self.user_key.len_of_encoding()
    }
}

impl<UserPublicKey> EncodableSync for OwnedInitialAuthorisationMsg<'_, UserPublicKey> where
    UserPublicKey: SubspaceId + EncodableKnownSize
{
}

/// Can be encoded to a bytestring to be signed for a new [`Delegation`] to an [`OwnedCapability`].
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#communal_handover)
pub struct OwnedHandover<
    'a,
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    cap: &'a OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >,
    area: &'a Area<MCL, MCC, MPL, UserPublicKey>,
    user: &'a UserPublicKey,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > Encodable
    for OwnedHandover<
        '_,
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if self.cap.delegations.is_empty() {
            let prev_area = Area::<MCL, MCC, MPL, UserPublicKey>::new_full();

            self.area.relative_encode(consumer, &prev_area).await?;
            self.cap.initial_authorisation.encode(consumer).await?;
            self.user.encode(consumer).await?;

            return Ok(());
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.cap.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        self.area.relative_encode(consumer, prev_area).await?;
        prev_signature.encode(consumer).await?;
        self.user.encode(consumer).await?;

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > EncodableKnownSize
    for OwnedHandover<
        '_,
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        if self.cap.delegations.is_empty() {
            let prev_area = Area::<MCL, MCC, MPL, UserPublicKey>::new_full();

            let area_len = self.area.relative_len_of_encoding(&prev_area);
            let initial_auth_len = self.cap.initial_authorisation.len_of_encoding();
            let user_len = self.user.len_of_encoding();

            return area_len + initial_auth_len + user_len;
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.cap.delegations.last().unwrap();
        let prev_area = last_delegation.area();
        let prev_signature = last_delegation.signature();

        let area_len = self.area.relative_len_of_encoding(prev_area);
        let prev_sig_len = prev_signature.len_of_encoding();
        let user_len = self.user.len_of_encoding();

        area_len + prev_sig_len + user_len
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > EncodableSync
    for OwnedHandover<
        '_,
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
}

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for OwnedCapability<MCL, MCC, MPL, SillyPublicKey, SillySig, SillyPublicKey, SillySig>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_key: SillyPublicKey = Arbitrary::arbitrary(u)?;

        let initial_authorisation: SillySig = Arbitrary::arbitrary(u)?;

        let user_key: SillyPublicKey = Arbitrary::arbitrary(u)?;
        let access_mode: AccessMode = Arbitrary::arbitrary(u)?;

        let init_auth = OwnedInitialAuthorisationMsg {
            access_mode,
            user_key: &user_key,
        };

        let message = init_auth.sync_encode_into_boxed_slice();

        namespace_key
            .verify(&message, &initial_authorisation)
            .map_err(|_| ArbitraryError::IncorrectFormat)?;

        let cap = Self {
            access_mode,
            initial_authorisation,
            namespace_key,
            user_key,
            delegations: Vec::new(),
        };

        let mut cap_with_delegations = cap.clone();
        let delegees: Vec<SillyPublicKey> = Arbitrary::arbitrary(u)?;

        let mut last_receiver = cap.receiver().clone();

        let mut prev_area = Area::new_full();

        for delegee in delegees {
            let area = arbitrary_included_area(&prev_area, u)?;

            prev_area = area;

            cap_with_delegations = cap_with_delegations
                .delegate(
                    &last_receiver.corresponding_secret_key(),
                    &delegee,
                    &prev_area,
                )
                .map_err(|_| ArbitraryError::IncorrectFormat)?;
            last_receiver = delegee;
        }

        Ok(cap_with_delegations)
    }
}

/// An [`OwnedCapability`] whose signatures have not been verified yet.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnverifiedOwnedCapability<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> {
    inner: OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >,
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
    UnverifiedOwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
{
    pub unsafe fn into_verifified_unchecked(
        self,
    ) -> OwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > {
        self.inner
    }

    pub fn from_verified(
        cap: OwnedCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
    ) -> Self {
        Self { inner: cap }
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > Encodable
    for UnverifiedOwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey,
    NamespaceSignature: Encodable + EncodableKnownSize,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: Encodable + EncodableKnownSize,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let inner = &self.inner;

        let delegations_length = inner.delegations.len() as u64;

        let mut header = 0;

        if inner.access_mode == AccessMode::Write {
            header |= 0b1100_0000;
        } else {
            header |= 0b1000_0000;
        }

        let delegations_length_tag = Tag::min_tag(delegations_length, TagWidth::six());
        header |= delegations_length_tag.data_at_offset(2);

        consumer.consume(header).await?;

        inner.namespace_key.encode(consumer).await?;
        inner.user_key.encode(consumer).await?;
        inner.initial_authorisation.encode(consumer).await?;
        CompactU64(delegations_length)
            .relative_encode(consumer, &delegations_length_tag.encoding_width())
            .await?;

        let mut prev_area = &Area::new_full();
        for delegation in inner.delegations.iter() {
            let current_area = delegation.area();

            current_area.relative_encode(consumer, prev_area).await?;
            delegation.user().encode(consumer).await?;
            delegation.signature().encode(consumer).await?;

            prev_area = current_area;
        }

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature: Encodable + EncodableKnownSize,
        UserPublicKey,
        UserSignature: Encodable + EncodableKnownSize,
    > EncodableKnownSize
    for UnverifiedOwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: Encodable + EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        let inner = &self.inner;

        let mut len = 0;
        len += 1; // header byte

        len += inner.namespace_key.len_of_encoding();
        len += inner.user_key.len_of_encoding();
        len += inner.initial_authorisation.len_of_encoding();
        len += Tag::min_tag(inner.delegations.len() as u64, TagWidth::six())
            .encoding_width()
            .as_usize();

        let mut prev_area = &Area::new_full();
        for delegation in inner.delegations.iter() {
            let current_area = delegation.area();

            len += current_area.relative_len_of_encoding(prev_area);
            len += delegation.user().len_of_encoding();
            len += delegation.signature().len_of_encoding();

            prev_area = current_area;
        }

        len
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > Decodable
    for UnverifiedOwnedCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey,
    NamespaceSignature: Decodable<ErrorReason = Blame>,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: Decodable<ErrorReason = Blame>,
{
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let access_mode = match header & 0b1100_0000 {
            0b1000_0000 => AccessMode::Read,
            0b1100_0000 => AccessMode::Write,
            _ => return Err(DecodeError::Other(Blame::TheirFault)),
        };
        let delegations_length_tag = Tag::from_raw(header, TagWidth::six(), 2);

        let namespace_key = NamespacePublicKey::decode(producer).await?;
        let user_key = UserPublicKey::decode(producer).await?;
        let initial_authorisation = NamespaceSignature::decode(producer).await?;
        let delegations_length = CompactU64::relative_decode(producer, &delegations_length_tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        let full_area = Area::new_full();
        let mut delegations: Vec<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>> =
            Vec::with_capacity(core::cmp::min(16, delegations_length as usize));
        for i in 0..delegations_length {
            let delegation_area = Area::relative_decode(
                producer,
                if i == 0 {
                    &full_area
                } else {
                    delegations.last().unwrap().area()
                },
            )
            .await?;
            let delegation_user = UserPublicKey::decode(producer).await?;
            let delegation_signature = UserSignature::decode(producer).await?;

            delegations.push(Delegation::new(
                delegation_area,
                delegation_user,
                delegation_signature,
            ));
        }

        Ok(UnverifiedOwnedCapability {
            inner: OwnedCapability {
                access_mode,
                namespace_key,
                user_key,
                initial_authorisation,
                delegations,
            },
        })
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for UnverifiedOwnedCapability<MCL, MCC, MPL, SillyPublicKey, SillySig, SillyPublicKey, SillySig>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            inner: Arbitrary::arbitrary(u)?,
        })
    }
}
