use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_data_model::SubspaceId;
use willow_encoding::{Decodable, DecodeError, Encodable};

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

impl Encodable for IdentityIdentifier {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.0.encode(consumer).await?;
        Ok(())
    }
}

impl Decodable for IdentityIdentifier {
    async fn decode<P>(producer: &mut P) -> Result<Self, DecodeError<P::Error>>
    where
        P: BulkProducer<Item = u8>,
    {
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
