use arbitrary::Arbitrary;
use meadowcap::IsCommunal;
use signature::{Error as SignatureError, Signer, Verifier};
use willow_data_model::{
    encoding::parameters_sync::Encodable,
    parameters::{NamespaceId, SubspaceId},
};

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
pub struct SillySig(SillyPublicKey, Box<[u8]>);

impl Signer<SillySig> for SillySecret {
    fn try_sign(&self, msg: &[u8]) -> Result<SillySig, signature::Error> {
        Ok(SillySig(SillyPublicKey(self.0), Box::from(msg)))
    }
}

impl Verifier<SillySig> for SillyPublicKey {
    fn verify(&self, msg: &[u8], signature: &SillySig) -> Result<(), SignatureError> {
        if &signature.0 != self {
            return Err(SignatureError::new());
        }

        let sig_msg = signature.1.as_ref();

        if msg != sig_msg {
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

impl Encodable for SillyPublicKey {
    fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: ufotofu::sync::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.0)?;

        Ok(())
    }
}

impl Encodable for SillySig {
    fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: ufotofu::sync::BulkConsumer<Item = u8>,
    {
        self.0.encode(consumer)?;

        consumer
            .bulk_consume_full_slice(self.1.as_ref())
            .map_err(|err| err.reason)?;

        Ok(())
    }
}
