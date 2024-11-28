use arbitrary::Arbitrary;
use meadowcap::IsCommunal;
use signature::{Error as SignatureError, Signer, Verifier};
use willow_data_model::{NamespaceId, SubspaceId};

/// A silly, trivial, insecure public key for fuzz testing.
#[derive(PartialEq, Eq, Debug, Arbitrary, Clone, Default, PartialOrd, Ord)]
pub struct SillyPublicKey(u8);

impl SillyPublicKey {
    pub fn corresponding_secret_key(&self) -> SillySecret {
        SillySecret(self.0)
    }
}

/// A silly, trivial, insecure secret key for fuzz testing.
/// The corresponding [`SillyPublicKey`] is the identity of the secret.
#[derive(PartialEq, Eq, Debug, Arbitrary)]
pub struct SillySecret(u8);

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

impl SubspaceId for SillyPublicKey {
    fn successor(&self) -> Option<Self> {
        if self.0 == 255 {
            return None;
        }

        Some(SillyPublicKey(self.0 + 1))
    }
}

impl IsCommunal for SillyPublicKey {
    fn is_communal(&self) -> bool {
        self.0 % 2 == 0
    }
}

use syncify::{syncify, syncify_replace};

pub mod encoding {
    use super::*;

    use ufotofu::{BulkConsumer, BulkProducer};

    use willow_encoding::produce_byte;
    use willow_encoding::DecodeError;
    use willow_encoding::{Decodable, Encodable, RelationDecodable};

    impl Encodable for SillyPublicKey {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            consumer.consume(self.0).await?;

            Ok(())
        }
    }

    impl Decodable for SillyPublicKey {
        async fn decode_canonical<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
        {
            let num = produce_byte(producer).await?;

            Ok(SillyPublicKey(num))
        }
    }

    impl Encodable for SillySig {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            consumer.consume(self.0).await?;

            Ok(())
        }
    }

    impl Decodable for SillySig {
        async fn decode_canonical<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
        {
            let num = produce_byte(producer).await?;

            Ok(SillySig(num))
        }
    }

    impl RelationDecodable for SillyPublicKey {}

    impl RelationDecodable for SillySig {}
}
