use ed25519_dalek::{SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH};
use meadowcap::{IsCommunal, McNamespacePublicKey};
use rand::rngs::OsRng;
use signature::Verifier;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync,
};
use willow_data_model::NamespaceId;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

use crate::SigningKey25;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct NamespaceId25(VerifyingKey);

/// An [ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) public key suitable for the Willow Data Model's [`NamespaceId`](https://willowprotocol.org/specs/data-model/index.html#NamespaceId) parameter, and Meadowcap's [`NamespacePublicKey`](https://willowprotocol.org/specs/meadowcap/index.html#NamespacePublicKey) parameter.
impl NamespaceId25 {
    /// Returns a new keypair where the component [`NamespaceId25`] is valid for use with *[communal namespaces](https://willowprotocol.org/specs/meadowcap/index.html#communal_namespace)*.
    ///
    /// Does not return a signing key as communal namespaces do not use namespace signatures.
    pub fn new_communal() -> Self {
        let mut csprng = OsRng;
        let mut signing_key: SigningKey = SigningKey::generate(&mut csprng);

        let mut maybe_communal = Self(signing_key.verifying_key());

        while !maybe_communal.is_communal() {
            signing_key = SigningKey::generate(&mut csprng);
            maybe_communal = Self(signing_key.verifying_key());
        }

        maybe_communal
    }

    /// Returns a new keypair where the component [`NamespaceId25`] is valid for use with *[owned namespaces](https://willowprotocol.org/specs/meadowcap/index.html#owned_namespace)*.
    pub fn new_owned() -> (Self, SigningKey25) {
        let mut csprng = OsRng;
        let signing_key: SigningKey = SigningKey::generate(&mut csprng);

        let mut maybe_owned = Self(signing_key.verifying_key());

        while maybe_owned.is_communal() {
            let signing_key: SigningKey = SigningKey::generate(&mut csprng);

            maybe_owned = Self(signing_key.verifying_key());
        }

        (maybe_owned, SigningKey25::new(signing_key))
    }
}

impl Verifier<crate::Signature25> for NamespaceId25 {
    fn verify(&self, msg: &[u8], signature: &crate::Signature25) -> Result<(), signature::Error> {
        self.0.verify_strict(msg, signature.inner())
    }
}

impl NamespaceId for NamespaceId25 {}
impl McNamespacePublicKey for NamespaceId25 {}

impl IsCommunal for NamespaceId25 {
    /// Returns true if the last bit of the first byte is zero.
    fn is_communal(&self) -> bool {
        let last_byte = *self.0.to_bytes().last().unwrap();

        0b1111_1110 & last_byte == 0b1111_1110
    }
}

impl Encodable for NamespaceId25 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let bytes_slice = self.0.to_bytes();

        consumer
            .bulk_consume_full_slice(&bytes_slice)
            .await
            .map_err(|err| err.into_reason())?;

        Ok(())
    }
}

impl EncodableSync for NamespaceId25 {}

impl EncodableKnownSize for NamespaceId25 {
    fn len_of_encoding(&self) -> usize {
        PUBLIC_KEY_LENGTH
    }
}

impl Decodable for NamespaceId25 {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice: [u8; PUBLIC_KEY_LENGTH] = [0; PUBLIC_KEY_LENGTH];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        match VerifyingKey::from_bytes(&slice) {
            Ok(key) => Ok(Self(key)),
            Err(_) => Err(DecodeError::Other(Blame::TheirFault)),
        }
    }
}

impl DecodableCanonic for NamespaceId25 {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice: [u8; PUBLIC_KEY_LENGTH] = [0; PUBLIC_KEY_LENGTH];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        match VerifyingKey::from_bytes(&slice) {
            Ok(key) => Ok(Self(key)),
            Err(_) => Err(DecodeError::Other(Blame::TheirFault)),
        }
    }
}

impl DecodableSync for NamespaceId25 {}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for NamespaceId25 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let is_communal = Arbitrary::arbitrary(u)?;

        if is_communal {
            Ok(Self::new_communal())
        } else {
            Ok(Self::new_owned().0)
        }
    }
}
