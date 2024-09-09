use arbitrary::Arbitrary;

use willow_data_model::NamespaceId;

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

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
pub(super) mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};
    use willow_encoding::DecodeError;
    #[syncify_replace(use willow_encoding::sync::{Encodable, Decodable};)]
    use willow_encoding::{Decodable, Encodable};

    impl Encodable for NamespaceIdentifier {
        async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
        where
            C: BulkConsumer<Item = u8>,
        {
            self.0.encode(consumer).await?;
            Ok(())
        }
    }

    impl Decodable for NamespaceIdentifier {
        async fn decode<P>(producer: &mut P) -> Result<Self, DecodeError<<P>::Error>>
        where
            P: BulkProducer<Item = u8>,
        {
            match Cinn25519PublicKey::decode(producer).await {
                Ok(pk) => Ok(Self(pk)),
                Err(err) => Err(err),
            }
        }
    }
}

impl NamespaceId for NamespaceIdentifier {}
