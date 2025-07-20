#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::CompactU64;
use signature::Verifier;
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable,
};
use willow_data_model::{
    grouping::{Area, AreaSubspace},
    NamespaceId, PrivateAreaContext, PrivateInterest, SubspaceId,
};

use crate::{
    AccessMode, CommunalCapability, Delegation, McNamespacePublicKey, McPublicUserKey,
    McEnumerationCapability, OwnedCapability, EnumerationDelegation,
};

#[derive(Debug)]
pub struct PersonalPrivateInterest<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: NamespaceId,
    UserPublicKey: SubspaceId,
> {
    private_interest: PrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    user_key: UserPublicKey,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: NamespaceId,
        UserPublicKey: SubspaceId,
    > PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>
{
    pub fn private_interest(&self) -> &PrivateInterest<MCL, MCC, MPL, N, UserPublicKey> {
        &self.private_interest
    }

    pub fn user_key(&self) -> &UserPublicKey {
        &self.user_key
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N: NamespaceId, S: SubspaceId>
    Arbitrary<'a> for PersonalPrivateInterest<MCL, MCC, MPL, N, S>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            private_interest: Arbitrary::arbitrary(u)?,
            user_key: Arbitrary::arbitrary(u)?,
        })
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: McNamespacePublicKey,
        UserPublicKey: McPublicUserKey<UserSignature>,
        UserSignature: EncodableSync + EncodableKnownSize + Clone,
    > RelativeEncodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>>
    for CommunalCapability<MCL, MCC, MPL, N, UserPublicKey, UserSignature>
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        // Check if this can be encoded relative to the private area
        // self is read
        if self.access_mode() != AccessMode::Read
        // private interest subspace is same as cap.user_key
            || r.private_interest.subspace_id() != self.progenitor()
        // private interest namespace is same
            || self.granted_namespace() != r.private_interest.namespace_id()
        // path of cap granted area is prefix of private interest path
            || !self
                .granted_area()
                .path()
                .is_prefix_of(r.private_interest.path())
        // receiver of cap is the user key of the interest
            || self.receiver() != &r.user_key
        {
            panic!("Tried to encode a CommunalCapability relative to a PersonalPrivateInterest it has no meaningful relation to.")
        }

        let ctxs: Vec<PrivateAreaContext<MCL, MCC, MPL, N, UserPublicKey>> = self
            .delegations()
            .map(|delegation| {
                PrivateAreaContext::new(r.private_interest.clone(), delegation.area().clone())
                    .unwrap()
            })
            .collect();

        let ctx_neg_one = match r.private_interest.subspace_id() {
            willow_data_model::grouping::AreaSubspace::Any => panic!("Communal capabilities are not compatible with personal private interests with any subspace!"),
            willow_data_model::grouping::AreaSubspace::Id(id) => {
              PrivateAreaContext::new(
                  r.private_interest.clone(),
                  Area::new_subspace(id.clone()),
              ).unwrap()
            },
        };

        CompactU64(self.delegations_len() as u64)
            .encode(consumer)
            .await?;

        for (i, delegation) in self.delegations().enumerate() {
            let ctx_to_use = if i == 0 { &ctx_neg_one } else { &ctxs[i - 1] };

            if i == self.delegations_len() - 1 {
                delegation
                    .area()
                    .relative_encode(consumer, ctx_to_use)
                    .await?;

                delegation.signature().encode(consumer).await?;
            } else {
                delegation
                    .area()
                    .relative_encode(consumer, ctx_to_use)
                    .await?;

                delegation.user().encode(consumer).await?;
                delegation.signature().encode(consumer).await?;
            }
        }

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: McNamespacePublicKey,
        UserPublicKey: McPublicUserKey<UserSignature>,
        UserSignature: EncodableSync + EncodableKnownSize + Clone + Decodable,
    > RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>, Blame>
    for CommunalCapability<MCL, MCC, MPL, N, UserPublicKey, UserSignature>
where
    Blame: From<UserPublicKey::ErrorReason> + From<UserSignature::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let delegations_len = CompactU64::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let mut last_ctx = match r.private_interest.subspace_id() {
            willow_data_model::grouping::AreaSubspace::Any => panic!("Communal capabilities are not compatible with personal private interests with any subspace!"),
            willow_data_model::grouping::AreaSubspace::Id(id) => {
              PrivateAreaContext::new(
                  r.private_interest.clone(),
                  Area::new_subspace(id.clone()),
              ).unwrap()
            },
        };

        let mut cap = match r.private_interest.subspace_id() {
            willow_data_model::grouping::AreaSubspace::Any => panic!("Communal capabilities are not compatible with personal private interests with any subspace!"),
            willow_data_model::grouping::AreaSubspace::Id(id) => CommunalCapability::new(
                r.private_interest.namespace_id().clone(),
                id.clone(),
                AccessMode::Read,
            )
            .unwrap(),
        };

        for i in 0..delegations_len.0 {
            if i == (delegations_len.0 - 1) {
                let area = Area::relative_decode(producer, &last_ctx)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let delegation = Delegation::new(area, r.user_key().clone(), sig);

                cap.append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?;
            } else {
                let area = Area::relative_decode(producer, &last_ctx)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let user_key = UserPublicKey::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                last_ctx =
                    PrivateAreaContext::new(r.private_interest.clone(), area.clone()).unwrap();

                let delegation = Delegation::new(area, user_key, sig);

                cap.append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?;
            }
        }

        Ok(cap)
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: McNamespacePublicKey + Verifier<NamespaceSignature>,
        NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
        UserPublicKey: McPublicUserKey<UserSignature> + Encodable,
        UserSignature: EncodableSync + EncodableKnownSize + Clone,
    > RelativeEncodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>>
    for OwnedCapability<MCL, MCC, MPL, N, NamespaceSignature, UserPublicKey, UserSignature>
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if let AreaSubspace::Id(id) = self.granted_area().subspace() {
            if r.private_interest().subspace_id() != id {
                panic!("Tried to encode a CommunalCapability relative to a PersonalPrivateInterest it has no meaningful relation to.")
            }
        }

        // Check if this can be encoded relative to the private area
        // self is read
        if self.access_mode() != AccessMode::Read
        // private interest subspace is same as cap.user_key
            || r.private_interest.subspace_id() != self.progenitor()
        // private interest namespace is same
            || self.granted_namespace() != r.private_interest.namespace_id()
        // path of cap granted area is prefix of private interest path
            || !self
                .granted_area()
                .path()
                .is_prefix_of(r.private_interest.path())
        // receiver of cap is the user key of the interest
            || self.receiver() != &r.user_key
        {
            panic!("Tried to encode a CommunalCapability relative to a PersonalPrivateInterest it has no meaningful relation to.")
        }

        let ctxs: Vec<PrivateAreaContext<MCL, MCC, MPL, N, UserPublicKey>> = self
            .delegations()
            .map(|delegation| {
                PrivateAreaContext::new(r.private_interest.clone(), delegation.area().clone())
                    .unwrap()
            })
            .collect();

        let ctx_neg_one = match r.private_interest.subspace_id() {
            willow_data_model::grouping::AreaSubspace::Any => panic!("Communal capabilities are not compatible with personal private interests with any subspace!"),
            willow_data_model::grouping::AreaSubspace::Id(id) => {
              PrivateAreaContext::new(
                  r.private_interest.clone(),
                  Area::new_subspace(id.clone()),
              ).unwrap()
            },
        };

        CompactU64(self.delegations_len() as u64)
            .encode(consumer)
            .await?;

        if self.delegations_len() > 0 {
            self.progenitor().encode(consumer).await?;
        }

        self.initial_authorisation().encode(consumer).await?;

        for (i, delegation) in self.delegations().enumerate() {
            let ctx_to_use = if i == 0 { &ctx_neg_one } else { &ctxs[i - 1] };

            if i == self.delegations_len() - 1 {
                delegation
                    .area()
                    .relative_encode(consumer, ctx_to_use)
                    .await?;

                delegation.signature().encode(consumer).await?;
            } else {
                delegation
                    .area()
                    .relative_encode(consumer, ctx_to_use)
                    .await?;

                delegation.user().encode(consumer).await?;
                delegation.signature().encode(consumer).await?;
            }
        }

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N: McNamespacePublicKey + Verifier<NamespaceSignature>,
        NamespaceSignature: Decodable + EncodableSync + EncodableKnownSize + Clone,
        UserPublicKey: McPublicUserKey<UserSignature>,
        UserSignature: EncodableSync + EncodableKnownSize + Clone + Decodable,
    > RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>, Blame>
    for OwnedCapability<MCL, MCC, MPL, N, NamespaceSignature, UserPublicKey, UserSignature>
where
    Blame: From<N::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserPublicKey::ErrorReason>
        + From<UserSignature::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let delegations_len = CompactU64::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let user_key = if delegations_len.0 > 0 {
            UserPublicKey::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            r.user_key().clone()
        };

        let initial_authorisation = NamespaceSignature::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let mut last_ctx = match r.private_interest.subspace_id() {
            willow_data_model::grouping::AreaSubspace::Any => panic!("Communal capabilities are not compatible with personal private interests with any subspace!"),
            willow_data_model::grouping::AreaSubspace::Id(id) => {
              PrivateAreaContext::new(
                  r.private_interest.clone(),
                  Area::new_subspace(id.clone()),
              ).unwrap()
            },
        };

        let mut cap = OwnedCapability::from_existing(
            r.private_interest().namespace_id().clone(),
            user_key,
            initial_authorisation,
            AccessMode::Read,
        )
        .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

        for i in 0..delegations_len.0 {
            if i == (delegations_len.0 - 1) {
                let area = Area::relative_decode(producer, &last_ctx)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let delegation = Delegation::new(area, r.user_key().clone(), sig);

                cap.append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?;
            } else {
                let area = Area::relative_decode(producer, &last_ctx)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let user_key = UserPublicKey::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                last_ctx =
                    PrivateAreaContext::new(r.private_interest.clone(), area.clone()).unwrap();

                let delegation = Delegation::new(area, user_key, sig);

                cap.append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?;
            }
        }

        Ok(cap)
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > RelativeEncodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>>
    for McEnumerationCapability<N, NamespaceSignature, UserPublicKey, UserSignature>
where
    N: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if *r.private_interest().subspace_id() != AreaSubspace::Any
            || r.private_interest().namespace_id() != self.granted_namespace()
        {
            panic!("Tried to encode a McSubspaceCapability relative to a PrivatePersonalInterest it has no relation to.")
        };

        CompactU64(self.delegations().len() as u64)
            .encode(consumer)
            .await?;

        self.initial_authorisation().encode(consumer).await?;

        if self.delegations().len() > 0 {
            self.progenitor().encode(consumer).await?;
        }

        for (i, delegation) in self.delegations().enumerate() {
            if i == self.delegations().len() - 1 {
                delegation.signature().encode(consumer).await?
            } else {
                delegation.user().encode(consumer).await?;
                delegation.signature().encode(consumer).await?;
            }
        }

        Ok(())
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        N,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > RelativeDecodable<PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>, Blame>
    for McEnumerationCapability<N, NamespaceSignature, UserPublicKey, UserSignature>
where
    N: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + Decodable,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + Decodable,
    Blame: From<N::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserPublicKey::ErrorReason>
        + From<UserSignature::ErrorReason>,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PersonalPrivateInterest<MCL, MCC, MPL, N, UserPublicKey>,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let delegations_len = CompactU64::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let initial_authorisation = NamespaceSignature::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let user_key = if delegations_len.0 > 0 {
            UserPublicKey::decode(producer)
                .await
                .map_err(DecodeError::map_other_from)?
        } else {
            r.user_key().clone()
        };

        let mut subspace_cap = Self::from_existing(
            r.private_interest().namespace_id().clone(),
            user_key,
            initial_authorisation,
        )
        .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

        for i in 0..delegations_len.0 {
            if i == delegations_len.0 - 1 {
                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let delegation = EnumerationDelegation::new(r.user_key().clone(), sig);

                subspace_cap
                    .append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?
            } else {
                let user_key = UserPublicKey::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let sig = UserSignature::decode(producer)
                    .await
                    .map_err(DecodeError::map_other_from)?;

                let delegation = EnumerationDelegation::new(user_key, sig);

                subspace_cap
                    .append_existing_delegation(delegation)
                    .map_err(|_| DecodeError::Other(Blame::TheirFault))?
            }
        }

        Ok(subspace_cap)
    }
}
