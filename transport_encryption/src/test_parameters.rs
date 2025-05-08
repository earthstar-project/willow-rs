use std::convert::Infallible;

use ufotofu_codec::{
    Decodable, DecodableCanonic, DecodableSync, Encodable, EncodableKnownSize, EncodableSync,
};

use crate::parameters::{AEADEncryptionKey, DiffieHellmanSecretKey, Hashing};

// The corresponding public key is equal to the secret key.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SillyDhSecretKey(pub u8);

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct SillyDhPublicKey(pub u8);

impl Encodable for SillyDhPublicKey {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.0).await?;

        Ok(())
    }
}

impl EncodableKnownSize for SillyDhPublicKey {
    fn len_of_encoding(&self) -> usize {
        1
    }
}

impl EncodableSync for SillyDhPublicKey {}

impl Decodable for SillyDhPublicKey {
    type ErrorReason = Infallible;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let num = producer.produce_item().await?;

        Ok(SillyDhPublicKey(num))
    }
}

impl DecodableCanonic for SillyDhPublicKey {
    type ErrorCanonic = Infallible;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::decode(producer).await
    }
}

impl DecodableSync for SillyDhPublicKey {}

impl DiffieHellmanSecretKey for SillyDhSecretKey {
    type PublicKey = SillyDhPublicKey;

    fn dh(&self, pk: &Self::PublicKey) -> Self::PublicKey {
        SillyDhPublicKey(self.0.wrapping_mul(pk.0))
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct SillyAead(pub u8);

impl AEADEncryptionKey<1, 1, false> for SillyAead {
    fn encrypt_inplace(
        &self,
        nonce: &[u8; 1],
        ad: &[u8],
        plaintext_with_additional_space: &mut [u8],
    ) {
        let len = plaintext_with_additional_space.len();
        for i in 0..(len - 1) {
            plaintext_with_additional_space[i] = plaintext_with_additional_space[i]
                .wrapping_add(self.0)
                .wrapping_add(nonce[0]);
        }
        plaintext_with_additional_space[len - 1] = if ad.len() == 0 { self.0 } else { ad[0] };
    }

    fn decrypt_inplace(
        &self,
        nonce: &[u8; 1],
        ad: &[u8],
        cyphertext_with_tag: &mut [u8],
    ) -> Result<(), ()> {
        let len = cyphertext_with_tag.len();
        for i in 0..(len - 1) {
            cyphertext_with_tag[i] = cyphertext_with_tag[i]
                .wrapping_sub(self.0)
                .wrapping_sub(nonce[0]);
        }

        let valid = if ad.len() == 0 {
            cyphertext_with_tag[len - 1] == self.0
        } else {
            cyphertext_with_tag[len - 1] == ad[0]
        };

        if valid {
            Ok(())
        } else {
            Err(())
        }
    }
}

pub struct SillyHash;

impl Hashing<1, 128, SillyAead> for SillyHash {
    fn hash(data: &[u8]) -> [u8; 1] {
        let mut acc = 0u8;
        for byte in data {
            acc = acc.wrapping_add(*byte);
        }
        [acc]
    }

    fn digest_to_aeadkey(digest: &[u8; 1]) -> SillyAead {
        SillyAead(digest[0])
    }
}
