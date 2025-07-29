use compact_u64::{CompactU64, EncodingWidth, Tag, TagWidth};
use signature::{Signer, Verifier};
use ufotofu::BulkProducer;
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable, RelativeEncodableKnownSize,
};
#[cfg(feature = "dev")]
use willow_data_model::grouping::arbitrary_included_area;
use willow_data_model::{grouping::Area, Entry, PrivateAreaContext, PrivateInterest};
use willow_encoding::is_bitflagged;

use crate::{
    AccessMode, Delegation, FailedDelegationError, InvalidDelegationError, McCapability,
    McNamespacePublicKey, McPublicUserKey,
};

#[cfg(feature = "dev")]
use crate::{SillyPublicKey, SillySig};

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
> {
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
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq,
{
    /// Creates a new communal capability granting access to the [`willow_data_model::SubspaceId`] corresponding to the given `UserPublicKey`.
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

    /// Returns a [`CommunalCapability`] without checking if any of the delegations are valid.
    ///
    /// # Safety
    /// Calling this method with an invalid delegation check is immediate undefined behaviour!
    pub unsafe fn new_unchecked(
        access_mode: AccessMode,
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        delegations: Vec<Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>,
    ) -> Self {
        Self {
            access_mode,
            namespace_key,
            user_key,
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
    )> for CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
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
            McCapability::Communal(communal_capability) => {
                self.longest_common_delegation_prefix_len(&communal_capability.delegations)
            }
            McCapability::Owned(_owned_capability) => 0,
        };

        let nice_hack = match &prior_cap {
            McCapability::Communal(_communal_capability) => shared + 1,
            McCapability::Owned(_owned_capability) => 0,
        };

        let nice_hack_tag = Tag::min_tag(nice_hack as u64, TagWidth::three());
        let del_len_tag = Tag::min_tag(self.delegations_len() as u64, TagWidth::four());

        let mut header = 0;

        header |= nice_hack_tag.data_at_offset(1);
        header |= del_len_tag.data_at_offset(4);

        consumer.consume(header).await?;

        CompactU64(nice_hack as u64)
            .relative_encode(consumer, &nice_hack_tag.encoding_width())
            .await?;

        CompactU64(self.delegations_len() as u64)
            .relative_encode(consumer, &del_len_tag.encoding_width())
            .await?;

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
    > for CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableKnownSize + EncodableSync + Decodable + Clone + PartialEq,
    Blame: From<UserPublicKey::ErrorReason> + From<UserSignature::ErrorReason>,
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

        if is_bitflagged(header, 0) {
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

        let mut decoded_cap = CommunalCapability::<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            UserPublicKey,
            UserSignature,
        >::new(
            entry.namespace_id().clone(),
            entry.subspace_id().clone(),
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
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
    cap: &'a CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>,
    area: &'a Area<MCL, MCC, MPL, UserPublicKey>,
    user: &'a UserPublicKey,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > Encodable
    for CommunalHandover<'_, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
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
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > EncodableKnownSize
    for CommunalHandover<'_, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
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
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        UserPublicKey,
        UserSignature,
    > EncodableSync
    for CommunalHandover<'_, MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize,
{
}

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, NamespacePublicKey> Arbitrary<'a>
    for CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, SillyPublicKey, SillySig>
where
    NamespacePublicKey: McNamespacePublicKey + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_key: NamespacePublicKey = Arbitrary::arbitrary(u)?;
        let user_key: SillyPublicKey = Arbitrary::arbitrary(u)?;
        let access_mode: AccessMode = Arbitrary::arbitrary(u)?;

        let mut prev_area = Area::new_subspace(user_key.clone());

        match Self::new(namespace_key, user_key, access_mode) {
            Ok(cap) => {
                let mut cap_with_delegations = cap.clone();
                let delegees: Vec<SillyPublicKey> = Arbitrary::arbitrary(u)?;

                let mut last_receiver = cap.receiver().clone();

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
            Err(_) => Err(ArbitraryError::IncorrectFormat),
        }
    }
}
