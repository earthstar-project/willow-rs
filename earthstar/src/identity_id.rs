use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_data_model::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decoder, Encoder},
    },
    parameters::SubspaceId,
};

use crate::cinn25519::{Cinn25519PublicKey, Shortname};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Arbitrary)]
pub struct IdentityIdentifier(Cinn25519PublicKey<4, 4>);

impl Default for IdentityIdentifier {
    fn default() -> Self {
        Self(Cinn25519PublicKey {
            shortname: Shortname(b"a000".to_vec()),
            underlying: [0u8; 32],
        })
    }
}

impl<C> Encoder<C> for IdentityIdentifier
where
    C: BulkConsumer<Item = u8>,
{
    async fn encode(&self, consumer: &mut C) -> Result<(), EncodingConsumerError<C::Error>> {
        self.0.encode(consumer).await?;
        Ok(())
    }
}

impl<C> Decoder<C> for IdentityIdentifier
where
    C: BulkProducer<Item = u8>,
{
    async fn decode(producer: &mut C) -> Result<Self, DecodeError<<C>::Error>> {
        match Cinn25519PublicKey::decode(producer).await {
            Ok(pk) => Ok(Self(pk)),
            Err(err) => Err(err),
        }
    }
}

impl SubspaceId for IdentityIdentifier {
    fn successor(&self) -> Option<Self> {
        unimplemented!("Port from earthstar-js")
    }
}
