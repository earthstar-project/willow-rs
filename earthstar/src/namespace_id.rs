use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_data_model::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decoder, Encoder},
    },
    parameters::NamespaceId,
};

use crate::cinn25519::{Cinn25519PublicKey, Shortname};

#[derive(PartialEq, Eq, Clone, Debug, Arbitrary)]
pub struct NamespaceIdentifier(pub Cinn25519PublicKey<1, 15>);

impl Default for NamespaceIdentifier {
    fn default() -> Self {
        Self(Cinn25519PublicKey {
            shortname: Shortname([b'a'].to_vec()),
            underlying: [0u8; 32],
        })
    }
}

impl<C> Encoder<C> for NamespaceIdentifier
where
    C: BulkConsumer<Item = u8>,
{
    async fn encode(&self, consumer: &mut C) -> Result<(), EncodingConsumerError<C::Error>> {
        self.0.encode(consumer).await?;
        Ok(())
    }
}

impl<C> Decoder<C> for NamespaceIdentifier
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

impl NamespaceId for NamespaceIdentifier {}
