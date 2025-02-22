use crate::{IsCommunal, McNamespacePublicKey, McPublicUserKey};
use arbitrary::Arbitrary;
use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::{NamespaceId, SubspaceId};

/// A silly, trivial, insecure public key for fuzz testing.
#[derive(PartialEq, Eq, Debug, Arbitrary, Clone, Default, PartialOrd, Ord)]
pub struct SillyPublicKey(bool);

impl SillyPublicKey {
    pub fn corresponding_secret_key(&self) -> SillySecret {
        SillySecret(self.0)
    }
}

/// A silly, trivial, insecure secret key for fuzz testing.
/// The corresponding [`SillyPublicKey`] is the identity of the secret.
#[derive(PartialEq, Eq, Debug, Arbitrary)]
pub struct SillySecret(bool);

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
#[derive(PartialEq, Eq, Debug, Arbitrary, Clone)]
pub struct SillySig(u8);

impl Signer<SillySig> for SillySecret {
    fn try_sign(&self, msg: &[u8]) -> Result<SillySig, signature::Error> {
        let first_msg_byte = msg.first().unwrap_or(&0x00);

        if self.0 {
            Ok(SillySig(first_msg_byte | 0b0000_0001))
        } else {
            Ok(SillySig(first_msg_byte & 0b1111_1110))
        }
    }
}

impl Verifier<SillySig> for SillyPublicKey {
    fn verify(&self, msg: &[u8], signature: &SillySig) -> Result<(), SignatureError> {
        let first_msg_byte = msg.first().unwrap_or(&0x00);

        let expected_sig = if self.0 {
            first_msg_byte | 0b0000_0001
        } else {
            first_msg_byte & 0b1111_1110
        };

        if signature.0 != expected_sig {
            return Err(SignatureError::new());
        };

        Ok(())
    }
}

impl NamespaceId for SillyPublicKey {}

impl McNamespacePublicKey for SillyPublicKey {}

impl SubspaceId for SillyPublicKey {
    fn successor(&self) -> Option<Self> {
        if self.0 {
            return None;
        }

        Some(SillyPublicKey(true))
    }
}

impl McPublicUserKey<SillySig> for SillyPublicKey {}

impl IsCommunal for SillyPublicKey {
    fn is_communal(&self) -> bool {
        self.0
    }
}

impl Encodable for SillyPublicKey {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let byte = if self.0 { 1 } else { 0 };

        consumer.consume(byte).await?;

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

        if num > 1 {
            Err(DecodeError::Other(Blame::TheirFault))
        } else if num == 0 {
            Ok(SillyPublicKey(false))
        } else {
            Ok(SillyPublicKey(true))
        }
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

        if num > 1 {
            Err(DecodeError::Other(Blame::TheirFault))
        } else if num == 0 {
            Ok(SillyPublicKey(false))
        } else {
            Ok(SillyPublicKey(true))
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
