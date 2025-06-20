use crate::{IsCommunal, McNamespacePublicKey, McPublicUserKey};
use arbitrary::Arbitrary;
use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::{NamespaceId, SubspaceId};

/// A silly, trivial, insecure public key for fuzz testing.
#[derive(PartialEq, Eq, Debug, Clone, Default, PartialOrd, Ord, Hash)]
pub struct SillyPublicKey(u8);

impl<'a> Arbitrary<'a> for SillyPublicKey {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(SillyPublicKey(u8::arbitrary(u).unwrap() & 0b0000_0011))
    }
}

impl SillyPublicKey {
    pub fn corresponding_secret_key(&self) -> SillySecret {
        SillySecret(self.0)
    }

    pub fn successor(&self) -> Option<Self> {
        if self.0 == 3 {
            None
        } else {
            Some(Self(self.0 + 1))
        }
    }
}

/// A silly, trivial, insecure secret key for fuzz testing.
/// The corresponding [`SillyPublicKey`] is the identity of the secret.
#[derive(PartialEq, Eq, Debug)]
pub struct SillySecret(u8);

impl<'a> Arbitrary<'a> for SillySecret {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(SillySecret(u8::arbitrary(u).unwrap() & 0b0000_0011))
    }
}

impl SillySecret {
    pub fn corresponding_public_key(&self) -> SillyPublicKey {
        SillyPublicKey(self.0)
    }
}

/// A silly, trivial, insecure keypair for fuzz testing.
/// The [`SillyPublicKey`]'s member must be the same as its [`SillySecret`] to be a valid keypair.
#[derive(PartialEq, Eq, Debug, Arbitrary)]
pub struct SillyKeypair(pub SillyPublicKey, pub SillySecret);

/// A silly, trivial, insecure signature for fuzz testing.
/// It's the public key followed by the message itself.
#[derive(PartialEq, Eq, Debug, Arbitrary, Clone, Hash)]
pub struct SillySig(u8);

impl Signer<SillySig> for SillySecret {
    fn try_sign(&self, msg: &[u8]) -> Result<SillySig, signature::Error> {
        let first_msg_byte = msg.first().unwrap_or(&0x00);

        Ok(SillySig(self.0 ^ first_msg_byte))
    }
}

impl Verifier<SillySig> for SillyPublicKey {
    fn verify(&self, msg: &[u8], signature: &SillySig) -> Result<(), SignatureError> {
        let first_msg_byte = msg.first().unwrap_or(&0x00);

        let expected_sig = self.0 ^ first_msg_byte;

        if signature.0 != expected_sig {
            return Err(SignatureError::new());
        }

        Ok(())
    }
}

impl NamespaceId for SillyPublicKey {}

impl McNamespacePublicKey for SillyPublicKey {}

impl SubspaceId for SillyPublicKey {
    fn successor(&self) -> Option<Self> {
        self.successor()
    }
}

impl McPublicUserKey<SillySig> for SillyPublicKey {}

impl IsCommunal for SillyPublicKey {
    fn is_communal(&self) -> bool {
        self.0 % 2 == 0
    }
}

impl Encodable for SillyPublicKey {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.0).await?;

        Ok(())
    }
}

impl EncodableKnownSize for SillyPublicKey {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for SillyPublicKey {}

impl Decodable for SillyPublicKey {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        Ok(SillyPublicKey(num & 0b0000_0011))
    }
}

impl DecodableCanonic for SillyPublicKey {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        if num < 4 {
            Ok(SillyPublicKey(num))
        } else {
            Err(DecodeError::Other(Blame::TheirFault))
        }
    }
}

impl Encodable for SillySig {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.0).await?;

        Ok(())
    }
}

impl EncodableKnownSize for SillySig {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for SillySig {}

impl Decodable for SillySig {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        Ok(SillySig(num))
    }
}

impl DecodableCanonic for SillySig {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        Ok(SillySig(num))
    }
}
