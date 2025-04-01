use ed25519_dalek::{SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH};
use meadowcap::{IsCommunal, McNamespacePublicKey};
use rand::rngs::OsRng;
use signature::Verifier;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::NamespaceId;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamespaceId25(VerifyingKey);

/// A Willowʼ25 NamespaceId, compatible with Meadowcap using the ed25519 signature scheme.
impl NamespaceId25 {
    /// Create a new communal namespace keypair,
    pub fn new_communal() -> (Self, SigningKey) {
        let mut csprng = OsRng;
        let mut signing_key: SigningKey = SigningKey::generate(&mut csprng);

        let mut maybe_communal = Self(signing_key.verifying_key());

        while !maybe_communal.is_communal() {
            signing_key = SigningKey::generate(&mut csprng);

            maybe_communal = Self(signing_key.verifying_key());
        }

        (maybe_communal, signing_key)
    }

    pub fn new_owned() -> (Self, SigningKey) {
        let mut csprng = OsRng;
        let signing_key: SigningKey = SigningKey::generate(&mut csprng);

        let mut maybe_owned = Self(signing_key.verifying_key());

        while maybe_owned.is_communal() {
            let signing_key: SigningKey = SigningKey::generate(&mut csprng);

            maybe_owned = Self(signing_key.verifying_key());
        }

        (maybe_owned, signing_key)
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
    /// Check if the last bit is zero
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
