use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu::sync::{consumer::IntoVec, Consumer};
use willow_data_model::{
    encoding::parameters_sync::Encodable,
    parameters::{NamespaceId, SubspaceId},
};

use crate::IsCommunal;

/// A capability that certifies read access to arbitrary [SubspaceIds](https://willowprotocol.org/specs/data-model/index.html#SubspaceId) at some unspecified non-empty [`willow_data_model::Path`].
///
/// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_capability)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McSubspaceCapability<
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    NamespaceSignature: Encodable + Clone,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    UserSignature: Encodable + Clone,
{
    namespace_key: NamespacePublicKey,
    user_key: UserPublicKey,
    initial_authorisation: NamespaceSignature,
    delegations: Vec<(UserPublicKey, UserSignature)>,
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
    McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    NamespaceSignature: Encodable + Clone,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    UserSignature: Encodable + Clone,
{
    /// Generate a new [`McSubspaceCapability`] for a given user, or return an error if the given namespace secret is incorrect.
    pub fn new<const MCL: usize, const MCC: usize, const MPL: usize, NamespaceSecret>(
        namespace_key: NamespacePublicKey,
        namespace_secret: NamespaceSecret,
        user_key: UserPublicKey,
    ) -> Result<
        McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>,
        SignatureError,
    >
    where
        NamespaceSecret: Signer<NamespaceSignature>,
    {
        let mut consumer = IntoVec::<u8>::new();

        // We can unwrap here because IntoVec::Error is ! (never)
        consumer.consume(0x2).unwrap();
        user_key.encode(&mut consumer).unwrap();
        let message = consumer.into_vec();

        let signature = namespace_secret.sign(&message);

        namespace_key.verify(&message, &signature)?;

        Ok(McSubspaceCapability {
            namespace_key: namespace_key.clone(),
            user_key,
            initial_authorisation: signature,
            delegations: Vec::new(),
        })
    }

    /// Instantiate an [`McSubspaceCapability`] using an existing authorisation (e.g. one received over the network), or return an error if the signature was not created by the namespace key.
    pub fn from_existing(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        initial_authorisation: NamespaceSignature,
    ) -> Result<Self, SignatureError> {
        let mut consumer = IntoVec::<u8>::new();

        // We can unwrap here because IntoVec::Error is ! (never)
        consumer.consume(0x2).unwrap();
        user_key.encode(&mut consumer).unwrap();
        let message = consumer.into_vec();

        namespace_key.verify(&message, &initial_authorisation)?;

        Ok(Self {
            namespace_key,
            user_key,
            initial_authorisation,
            delegations: Vec::new(),
        })
    }

    /// The user to whom this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#owned_cap_receiver)
    pub fn receiver(&self) -> &UserPublicKey {
        if self.delegations.is_empty() {
            return &self.user_key;
        }

        // We can unwrap here because we know delegations isn't empty.
        let last_delegation = self.delegations.last().unwrap();
        &last_delegation.0
    }

    /// The [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_cap_receiver)
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        &self.namespace_key
    }

    /// Delegate this subspace capability to a new [`UserPublicKey`].
    /// Will fail if the given secret key does not correspond to the subspace capability's [receiver](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_receiver).
    pub fn delegate<UserSecretKey>(
        &self,
        secret_key: &UserSecretKey,
        new_user: &UserPublicKey,
    ) -> Result<Self, SignatureError>
    where
        UserSecretKey: Signer<UserSignature>,
    {
        let prev_user = self.receiver();

        let handover = self.handover(new_user);
        let signature = secret_key.sign(&handover);

        prev_user.verify(&handover, &signature)?;

        let mut new_delegations = self.delegations.clone();

        new_delegations.push((new_user.clone(), signature));

        Ok(Self {
            namespace_key: self.namespace_key.clone(),
            initial_authorisation: self.initial_authorisation.clone(),
            user_key: self.user_key.clone(),
            delegations: new_delegations,
        })
    }

    /// Append an existing delegation to an existing capability, or return an error if the delegation is invalid.
    pub fn append_existing_delegation(
        &mut self,
        delegation: (UserPublicKey, UserSignature),
    ) -> Result<(), SignatureError> {
        let new_user = &delegation.0;
        let new_sig = &delegation.1;

        let handover = self.handover(new_user);

        let prev_receiver = self.receiver();

        prev_receiver.verify(&handover, new_sig)?;

        self.delegations.push(delegation);

        Ok(())
    }

    /// Return a slice of all [`Delegation`]s made to this capability.
    pub fn delegations(&self) -> impl Iterator<Item = &(UserPublicKey, UserSignature)> {
        self.delegations.iter()
    }

    /// A bytestring to be signed for a new subspace capability delegation.
    ///
    /// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_handover)
    fn handover(&self, new_user: &UserPublicKey) -> Box<[u8]> {
        let mut consumer = IntoVec::<u8>::new();

        if self.delegations.is_empty() {
            // We can safely unwrap all these encodings as IntoVec's error is the never type.

            self.initial_authorisation.encode(&mut consumer).unwrap();
            new_user.encode(&mut consumer).unwrap();

            return consumer.into_vec().into();
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.delegations.last().unwrap();

        let prev_signature = &last_delegation.1;
        // We can safely unwrap all these encodings as IntoVec's error is the never type.
        prev_signature.encode(&mut consumer).unwrap();
        new_user.encode(&mut consumer).unwrap();

        consumer.into_vec().into()
    }
}

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
pub(super) mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    #[syncify_replace(use willow_data_model::encoding::parameters_sync::{Encodable, Decodable};)]
    use willow_data_model::encoding::parameters::{Decodable, Encodable};

    use willow_data_model::encoding::{
        compact_width::CompactWidth, error::DecodeError,
        parameters_sync::Encodable as EncodableSync,
    };

    #[syncify_replace(use willow_data_model::encoding::bytes::encoding_sync::produce_byte;)]
    use willow_data_model::encoding::bytes::encoding::produce_byte;

    #[syncify_replace(
      use willow_data_model::encoding::compact_width::encoding_sync::{encode_compact_width_be, decode_compact_width_be};
  )]
    use willow_data_model::encoding::compact_width::encoding::{
        decode_compact_width_be, encode_compact_width_be,
    };

    impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Encodable
        for McSubspaceCapability<
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >
    where
        NamespacePublicKey:
            NamespaceId + EncodableSync + Encodable + Verifier<NamespaceSignature> + IsCommunal,
        NamespaceSignature: EncodableSync + Encodable + Clone,
        UserPublicKey: SubspaceId + EncodableSync + Encodable + Verifier<UserSignature>,
        UserSignature: EncodableSync + Encodable + Clone,
    {
        async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
        where
            C: BulkConsumer<Item = u8>,
        {
            let mut header = 0;

            let delegations_count = self.delegations.len();

            if delegations_count >= 4294967296 {
                header |= 0b1111_1111;
            } else if delegations_count >= 65536 {
                header |= 0b1111_1110;
            } else if delegations_count >= 256 {
                header |= 0b1111_1101;
            } else if delegations_count >= 60 {
                header |= 0b1111_1100;
            } else {
                header |= delegations_count as u8;
            }

            consumer.consume(header).await?;

            Encodable::encode(&self.namespace_key, consumer).await?;
            Encodable::encode(&self.user_key, consumer).await?;
            Encodable::encode(&self.initial_authorisation, consumer).await?;

            if delegations_count >= 60 {
                encode_compact_width_be(delegations_count as u64, consumer).await?;
            }

            for delegation in self.delegations.iter() {
                Encodable::encode(&delegation.0, consumer).await?;
                Encodable::encode(&delegation.1, consumer).await?;
            }

            Ok(())
        }
    }

    impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Decodable
        for McSubspaceCapability<
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >
    where
        NamespacePublicKey:
            NamespaceId + EncodableSync + Decodable + Verifier<NamespaceSignature> + IsCommunal,
        NamespaceSignature: EncodableSync + Decodable + Clone,
        UserPublicKey: SubspaceId + EncodableSync + Decodable + Verifier<UserSignature>,
        UserSignature: EncodableSync + Decodable + Clone,
    {
        async fn decode<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
        where
            P: BulkProducer<Item = u8>,
        {
            // TODO: Strict encoding relations - this decoder will not throw if the number of delegations claimed is not the same as the number of delegations decoded.

            let header = produce_byte(producer).await?;

            let namespace_key = NamespacePublicKey::decode(producer).await?;
            let user_key = UserPublicKey::decode(producer).await?;
            let initial_authorisation = NamespaceSignature::decode(producer).await?;

            let mut base_cap = Self::from_existing(namespace_key, user_key, initial_authorisation)
                .map_err(|_| DecodeError::InvalidInput)?;

            let delegations_to_decode = if header == 0b1111_1111 {
                decode_compact_width_be(CompactWidth::Eight, producer).await?
            } else if header == 0b1111_1110 {
                decode_compact_width_be(CompactWidth::Four, producer).await?
            } else if header == 0b1111_1101 {
                decode_compact_width_be(CompactWidth::Two, producer).await?
            } else if header == 0b1111_1100 {
                decode_compact_width_be(CompactWidth::One, producer).await?
            } else {
                header as u64
            };

            for _ in 0..delegations_to_decode {
                let user = UserPublicKey::decode(producer).await?;
                let signature = UserSignature::decode(producer).await?;

                base_cap
                    .append_existing_delegation((user, signature))
                    .map_err(|_| DecodeError::InvalidInput)?;
            }

            Ok(base_cap)
        }
    }
}

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

#[cfg(feature = "dev")]
impl<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Arbitrary<'a>
    for McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey:
        NamespaceId + Encodable + IsCommunal + Arbitrary<'a> + Verifier<NamespaceSignature>,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature> + Arbitrary<'a>,
    NamespaceSignature: Encodable + Clone + Arbitrary<'a>,
    UserSignature: Encodable + Clone,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_key: NamespacePublicKey = Arbitrary::arbitrary(u)?;
        let user_key: UserPublicKey = Arbitrary::arbitrary(u)?;
        let initial_authorisation: NamespaceSignature = Arbitrary::arbitrary(u)?;

        let mut consumer = IntoVec::<u8>::new();

        // We can unwrap here because IntoVec::Error is ! (never)
        consumer.consume(0x2).unwrap();
        user_key.encode(&mut consumer).unwrap();
        let message = consumer.into_vec();

        namespace_key
            .verify(&message, &initial_authorisation)
            .map_err(|_| ArbitraryError::IncorrectFormat)?;

        Ok(Self {
            namespace_key,
            user_key,
            initial_authorisation,
            delegations: Vec::new(),
        })
    }
}
