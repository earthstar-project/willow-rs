use either::Either;
use signature::Verifier;
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
    RelativeDecodable, RelativeEncodable,
};
use willow_data_model::{
    grouping::Area, AuthorisationToken, AuthorisedEntry, Entry, PayloadDigest, TrustedDecodable,
    TrustedRelativeDecodable,
};

use crate::{
    mc_capability::McCapability, AccessMode, CommunalCapability, McNamespacePublicKey,
    McPublicUserKey, OwnedCapability,
};

#[cfg(feature = "dev")]
use crate::{SillyPublicKey, SillySig};

/// To be used as the [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) parameter for the [Willow data model](https://willowprotocol.org/specs/data-model).
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#MeadowcapAuthorisationToken)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McAuthorisationToken<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> {
    /// Certifies that an Entry may be written.
    pub capability: McCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >,
    /// Proves that the [`willow_data_model::Entry`] was created by the [receiver](https://willowprotocol.org/specs/meadowcap/index.html#cap_receiver) of the [capability](https://willowprotocol.org/specs/meadowcap/index.html#mcat_cap).
    pub signature: UserSignature,
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
    McAuthorisationToken<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
{
    /// Returns a new [`McAuthorisationToken`] using the given [`McCapability`] and `UserSignature`.
    ///
    /// Does **not** verify the signature's validity.
    pub fn new(
        capability: McCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        signature: UserSignature,
    ) -> Self {
        Self {
            capability,
            signature,
        }
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
    > AuthorisationToken<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>
    for McAuthorisationToken<
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
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + core::fmt::Debug,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + core::fmt::Debug + PartialEq,
    PD: PayloadDigest + EncodableSync + EncodableKnownSize,
{
    fn is_authorised_write(
        &self,
        entry: &Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
    ) -> bool {
        match self.capability.access_mode() {
            AccessMode::Read => return false,
            AccessMode::Write => {}
        }

        if !self.capability.granted_area().includes_entry(entry) {
            return false;
        }

        let message = entry.sync_encode_into_boxed_slice();

        if self
            .capability
            .receiver()
            .verify(&message, &self.signature)
            .is_err()
        {
            return false;
        }

        true
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
    for McAuthorisationToken<
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
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + PartialEq,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        self.capability
            .relative_encode(consumer, &Area::new_full())
            .await?;
        self.signature.encode(consumer).await?;

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
    > Decodable
    for McAuthorisationToken<
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
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync
        + EncodableKnownSize
        + Clone
        + Decodable<ErrorReason = Blame>
        + DecodableCanonic<ErrorCanonic = Blame>,
    UserSignature: EncodableSync
        + EncodableKnownSize
        + Clone
        + Decodable<ErrorReason = Blame>
        + DecodableCanonic<ErrorCanonic = Blame>
        + PartialEq,
{
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let capability = McCapability::relative_decode(producer, &Area::new_full()).await?;
        let signature = UserSignature::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(McAuthorisationToken {
            capability,
            signature,
        })
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
    > TrustedDecodable
    for McAuthorisationToken<
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
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + Decodable<ErrorReason = Blame>,
    UserSignature:
        PartialEq + EncodableSync + EncodableKnownSize + Clone + Decodable<ErrorReason = Blame>,
{
    async unsafe fn trusted_decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
    {
        let capability = McCapability::trusted_relative_decode(producer, &Area::new_full()).await?;
        let signature = UserSignature::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        Ok(McAuthorisationToken {
            capability,
            signature,
        })
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
        AuthorisedEntry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD, Self>,
        Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
    )>
    for McAuthorisationToken<
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
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone + Decodable + PartialEq,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone + Decodable + PartialEq,
    Blame: From<NamespacePublicKey::ErrorReason>
        + From<UserPublicKey::ErrorReason>
        + From<UserPublicKey::ErrorCanonic>
        + From<NamespaceSignature::ErrorReason>
        + From<UserSignature::ErrorReason>,
    PD: Clone,
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(
            AuthorisedEntry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD, Self>,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let (prev, entry) = r;

        if self.capability.granted_namespace() != entry.namespace_id()
            || !self.capability.granted_area().includes_entry(entry)
        {
            panic!(
                "Tried to encode an Authorisation token relative to an entry it cannot authorise"
            )
        }

        match &self.capability {
            McCapability::Communal(communal_capability) => {
                let pair = (prev.token().capability.clone(), entry.clone());

                communal_capability.relative_encode(consumer, &pair).await?
            }
            McCapability::Owned(owned_capability) => {
                let pair = (prev.token().capability.clone(), entry.clone());

                owned_capability.relative_encode(consumer, &pair).await?
            }
        }

        self.signature.encode(consumer).await?;

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
            AuthorisedEntry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD, Self>,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
        Blame,
    >
    for McAuthorisationToken<
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
    UserSignature: EncodableKnownSize + EncodableSync + Decodable + Clone + PartialEq,
    Blame: From<UserPublicKey::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserSignature::ErrorReason>,
    PD: Clone,
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(
            AuthorisedEntry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD, Self>,
            Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        ),
    ) -> Result<Self, DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        match producer.expose_items().await {
            Ok(Either::Left(exposed_bytes)) => {
                if exposed_bytes[0] & 0b1000_0000 == 0 {
                    let (prev, entry) = r;

                    let prior_cap = prev.token().capability.clone();

                    let cap =
                        CommunalCapability::relative_decode(producer, &(prior_cap, entry.clone()))
                            .await?;

                    let signature = UserSignature::decode(producer)
                        .await
                        .map_err(DecodeError::map_other_from)?;

                    Ok(Self {
                        capability: McCapability::Communal(cap),
                        signature,
                    })
                } else {
                    let (prev, entry) = r;

                    let prior_cap = prev.token().capability.clone();

                    let cap =
                        OwnedCapability::relative_decode(producer, &(prior_cap, entry.clone()))
                            .await?;

                    let signature = UserSignature::decode(producer)
                        .await
                        .map_err(DecodeError::map_other_from)?;

                    Ok(Self {
                        capability: McCapability::Owned(cap),
                        signature,
                    })
                }
            }
            _ => Err(DecodeError::Other(Blame::OurFault)),
        }
    }
}

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for McAuthorisationToken<MCL, MCC, MPL, SillyPublicKey, SillySig, SillyPublicKey, SillySig>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let capability: McCapability<
            MCL,
            MCC,
            MPL,
            SillyPublicKey,
            SillySig,
            SillyPublicKey,
            SillySig,
        > = Arbitrary::arbitrary(u)?;

        let signature: SillySig = Arbitrary::arbitrary(u)?;

        Ok(Self {
            capability,
            signature,
        })
    }
}
