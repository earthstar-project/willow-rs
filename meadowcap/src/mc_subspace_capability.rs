use compact_u64::CompactU64;
use compact_u64::Tag;
use compact_u64::TagWidth;
use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu_codec::Blame;
use ufotofu_codec::Decodable;
use ufotofu_codec::DecodableCanonic;
use ufotofu_codec::DecodeError;
use ufotofu_codec::Encodable;
use ufotofu_codec::EncodableKnownSize;
use ufotofu_codec::EncodableSync;
use ufotofu_codec::RelativeDecodableCanonic;
use ufotofu_codec::RelativeEncodable;
use ufotofu_codec::RelativeEncodableKnownSize;
use willow_data_model::SubspaceId;

use crate::McNamespacePublicKey;
use crate::McPublicUserKey;
use crate::SillyPublicKey;
use crate::SillySig;

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

/// A [delegation](https://willowprotocol.org/specs/pai/index.html#subspace_cap_delegations) of read access for arbitrary `SubspaceId`s to a `UserPublicKey`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct SubspaceDelegation<UserPublicKey, UserSignature>
where
    UserPublicKey: SubspaceId,
{
    user: UserPublicKey,
    signature: UserSignature,
}

impl<UserPublicKey, UserSignature> SubspaceDelegation<UserPublicKey, UserSignature>
where
    UserPublicKey: SubspaceId,
{
    pub fn new(user: UserPublicKey, signature: UserSignature) -> Self {
        Self { user, signature }
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

/// A capability that certifies read access to arbitrary [SubspaceIds](https://willowprotocol.org/specs/data-model/index.html#SubspaceId) at some unspecified non-empty [`willow_data_model::Path`].
///
/// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_capability)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct McSubspaceCapability<
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: Encodable + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: Encodable + Clone,
{
    namespace_key: NamespacePublicKey,
    user_key: UserPublicKey,
    initial_authorisation: NamespaceSignature,
    delegations: Vec<SubspaceDelegation<UserPublicKey, UserSignature>>,
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
    McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,

    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    /// Creates a new [`McSubspaceCapability`] for a given user, or return an error if the given namespace secret is incorrect.
    pub fn new<NamespaceSecret>(
        namespace_key: NamespacePublicKey,
        namespace_secret: &NamespaceSecret,
        user_key: UserPublicKey,
    ) -> Result<
        McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>,
        SignatureError,
    >
    where
        NamespaceSecret: Signer<NamespaceSignature>,
    {
        let init_auth = SubspaceInitialAuthorisationMsg(&user_key);

        let message = init_auth.sync_encode_into_boxed_slice();

        let signature = namespace_secret.sign(&message);

        namespace_key.verify(&message, &signature)?;

        Ok(McSubspaceCapability {
            namespace_key: namespace_key.clone(),
            user_key,
            initial_authorisation: signature,
            delegations: Vec::new(),
        })
    }

    /// Creates an [`McSubspaceCapability`] using an existing authorisation (e.g. one received over the network), or return an error if the signature was not created by the namespace key.
    pub fn from_existing(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        initial_authorisation: NamespaceSignature,
    ) -> Result<Self, SignatureError> {
        let init_auth = SubspaceInitialAuthorisationMsg(&user_key);

        let message = init_auth.sync_encode_into_boxed_slice();

        namespace_key.verify(&message, &initial_authorisation)?;
        Ok(Self {
            namespace_key,
            user_key,
            initial_authorisation,
            delegations: Vec::new(),
        })
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
        last_delegation.user()
    }

    /// Returns the public key of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    ///
    /// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_cap_receiver)
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        &self.namespace_key
    }

    /// Delegates this subspace capability to a new `UserPublicKey`.
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

        let handover = SubspaceHandover {
            subspace_cap: self,
            user: new_user,
        };

        let handover_enc = handover.sync_encode_into_boxed_slice();

        let signature = secret_key.sign(&handover_enc);

        prev_user.verify(&handover_enc, &signature)?;

        let mut new_delegations = self.delegations.clone();

        new_delegations.push(SubspaceDelegation::new(new_user.clone(), signature));

        Ok(Self {
            namespace_key: self.namespace_key.clone(),
            initial_authorisation: self.initial_authorisation.clone(),
            user_key: self.user_key.clone(),
            delegations: new_delegations,
        })
    }

    /// Appends an existing delegation to an existing capability, or return an error if the delegation is invalid.
    pub fn append_existing_delegation(
        &mut self,
        delegation: SubspaceDelegation<UserPublicKey, UserSignature>,
    ) -> Result<(), SignatureError> {
        let new_user = &delegation.user();
        let new_sig = &delegation.signature();

        let handover = SubspaceHandover {
            subspace_cap: self,
            user: new_user,
        };

        let handover_enc = handover.sync_encode_into_boxed_slice();

        let prev_receiver = self.receiver();

        prev_receiver.verify(&handover_enc, new_sig)?;

        self.delegations.push(delegation);

        Ok(())
    }

    /// Returns a slice of all [`SubspaceDelegation`]s made to this capability.
    pub fn delegations(
        &self,
    ) -> impl ExactSizeIterator<Item = &SubspaceDelegation<UserPublicKey, UserSignature>> {
        self.delegations.iter()
    }

    pub fn initial_authorisation(&self) -> &NamespaceSignature {
        &self.initial_authorisation
    }

    pub fn progenitor(&self) -> &UserPublicKey {
        &self.user_key
    }
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Encodable
    for McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: Encodable + Encodable + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: Encodable + Encodable + Clone,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let mut header = 0;

        let delegations_count = self.delegations.len() as u64;

        let tag = Tag::min_tag(delegations_count, TagWidth::eight());

        header |= tag.data();

        consumer.consume(header).await?;

        Encodable::encode(&self.namespace_key, consumer).await?;
        Encodable::encode(&self.user_key, consumer).await?;
        Encodable::encode(&self.initial_authorisation, consumer).await?;

        CompactU64(delegations_count)
            .relative_encode(consumer, &tag.encoding_width())
            .await?;

        for delegation in self.delegations.iter() {
            Encodable::encode(delegation.user(), consumer).await?;
            Encodable::encode(delegation.signature(), consumer).await?;
        }

        Ok(())
    }
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Decodable
    for McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + DecodableCanonic + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + DecodableCanonic + Clone,
    Blame: From<NamespacePublicKey::ErrorReason>
        + From<UserPublicKey::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserSignature::ErrorReason>
        + From<NamespacePublicKey::ErrorCanonic>
        + From<UserPublicKey::ErrorCanonic>
        + From<NamespaceSignature::ErrorCanonic>
        + From<UserSignature::ErrorCanonic>,
{
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode_canonic(producer).await
    }
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> DecodableCanonic
    for McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + DecodableCanonic + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + DecodableCanonic + Clone,
    Blame: From<NamespacePublicKey::ErrorReason>
        + From<UserPublicKey::ErrorReason>
        + From<NamespaceSignature::ErrorReason>
        + From<UserSignature::ErrorReason>
        + From<NamespacePublicKey::ErrorCanonic>
        + From<UserPublicKey::ErrorCanonic>
        + From<NamespaceSignature::ErrorCanonic>
        + From<UserSignature::ErrorCanonic>,
{
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let header = producer.produce_item().await?;

        let namespace_key = NamespacePublicKey::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let user_key = UserPublicKey::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let initial_authorisation = NamespaceSignature::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

        let mut base_cap = Self::from_existing(namespace_key, user_key, initial_authorisation)
            .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

        let tag = Tag::from_raw(header, TagWidth::eight(), 0);

        let delegations_to_decode = CompactU64::relative_decode_canonic(producer, &tag)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;

        /*
        let expected_tag = Tag::min_tag(delegations_to_decode, TagWidth::eight());

        if expected_tag != tag {
            return Err(DecodeError::Other(Blame::TheirFault));
        }
        */

        let mut delegations_decoded = 0;

        for _ in 0..delegations_to_decode {
            let user = UserPublicKey::decode_canonic(producer)
                .await
                .map_err(DecodeError::map_other_from)?;
            let signature = UserSignature::decode_canonic(producer)
                .await
                .map_err(DecodeError::map_other_from)?;
            let delegation = SubspaceDelegation::new(user, signature);

            base_cap
                .append_existing_delegation(delegation)
                .map_err(|_| DecodeError::Other(Blame::TheirFault))?;

            delegations_decoded += 1;
        }

        if delegations_decoded != delegations_to_decode {
            return Err(DecodeError::Other(Blame::TheirFault));
        }

        Ok(base_cap)
    }
}

impl<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> EncodableKnownSize
    for McSubspaceCapability<NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableKnownSize + Clone,
{
    fn len_of_encoding(&self) -> usize {
        let delegations_count = self.delegations.len() as u64;

        let namespace_len = self.namespace_key.len_of_encoding();
        let user_len = self.user_key.len_of_encoding();
        let init_auth_len = self.initial_authorisation.len_of_encoding();

        let tag = Tag::min_tag(delegations_count, TagWidth::eight());

        let delegation_count_len =
            CompactU64(delegations_count).relative_len_of_encoding(&tag.encoding_width());

        let mut delegations_len = 0;

        for delegation in self.delegations.iter() {
            delegations_len += delegation.user.len_of_encoding();
            delegations_len += delegation.signature().len_of_encoding();
        }

        1 + namespace_len + user_len + init_auth_len + delegation_count_len + delegations_len
    }
}

struct SubspaceInitialAuthorisationMsg<'a, UserPublicKey>(&'a UserPublicKey)
where
    UserPublicKey: EncodableSync + EncodableKnownSize;

impl<'a, UserPublicKey> Encodable for SubspaceInitialAuthorisationMsg<'a, UserPublicKey>
where
    UserPublicKey: EncodableSync + EncodableKnownSize,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(0x2).await?;
        self.0.encode(consumer).await?;

        Ok(())
    }
}

impl<'a, UserPublicKey> EncodableKnownSize for SubspaceInitialAuthorisationMsg<'a, UserPublicKey>
where
    UserPublicKey: EncodableSync + EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        1 + self.0.len_of_encoding()
    }
}

impl<'a, UserPublicKey> EncodableSync for SubspaceInitialAuthorisationMsg<'a, UserPublicKey> where
    UserPublicKey: EncodableSync + EncodableKnownSize
{
}

/// Can be encoded to a bytestring to be signed for a new subspace capability delegation.
///
/// [Definition](https://willowprotocol.org/specs/pai/index.html#subspace_handover)
struct SubspaceHandover<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    subspace_cap: &'a McSubspaceCapability<
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >,
    user: &'a UserPublicKey,
}

impl<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> Encodable
    for SubspaceHandover<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        if self.subspace_cap.delegations.is_empty() {
            // We can safely unwrap all these encodings as IntoVec's error is the never type.

            self.subspace_cap
                .initial_authorisation
                .encode(consumer)
                .await?;
            self.user.encode(consumer).await?;

            return Ok(());
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.subspace_cap.delegations.last().unwrap();

        let prev_signature = &last_delegation.signature();
        // We can safely unwrap all these encodings as IntoVec's error is the never type.
        prev_signature.encode(consumer).await?;
        self.user.encode(consumer).await?;

        Ok(())
    }
}

impl<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> EncodableKnownSize
    for SubspaceHandover<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    fn len_of_encoding(&self) -> usize {
        if self.subspace_cap.delegations.is_empty() {
            // We can safely unwrap all these encodings as IntoVec's error is the never type.

            let init_auth_len = self.subspace_cap.initial_authorisation.len_of_encoding();
            let user_len = self.user.len_of_encoding();

            return init_auth_len + user_len;
        }

        // We can unwrap here because we know that self.delegations is not empty.
        let last_delegation = self.subspace_cap.delegations.last().unwrap();

        let prev_signature = &last_delegation.signature();
        // We can safely unwrap all these encodings as IntoVec's error is the never type.

        let prev_sig_len = prev_signature.len_of_encoding();
        let user_len = self.user.len_of_encoding();

        prev_sig_len + user_len
    }
}

impl<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature> EncodableSync
    for SubspaceHandover<'a, NamespacePublicKey, NamespaceSignature, UserPublicKey, UserSignature>
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserPublicKey: McPublicUserKey<UserSignature>,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a>
    for McSubspaceCapability<SillyPublicKey, SillySig, SillyPublicKey, SillySig>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_key: SillyPublicKey = Arbitrary::arbitrary(u)?;
        let user_key: SillyPublicKey = Arbitrary::arbitrary(u)?;
        let initial_authorisation: SillySig = Arbitrary::arbitrary(u)?;

        let init_auth = SubspaceInitialAuthorisationMsg(&user_key);
        let message = init_auth.sync_encode_into_boxed_slice();

        namespace_key
            .verify(&message, &initial_authorisation)
            .map_err(|_| ArbitraryError::IncorrectFormat)?;

        let mut cap = Self {
            namespace_key,
            user_key: user_key.clone(),
            initial_authorisation,
            delegations: Vec::new(),
        };

        let delegees: Vec<SillyPublicKey> = Arbitrary::arbitrary(u)?;

        let mut last_receiver = user_key;

        for delegee in delegees {
            cap = cap
                .delegate(&last_receiver.corresponding_secret_key(), &delegee)
                .map_err(|_| ArbitraryError::IncorrectFormat)?;

            last_receiver = delegee;
        }

        Ok(cap)
    }
}
