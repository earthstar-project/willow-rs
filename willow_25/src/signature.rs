use signature::Signer;
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, Encodable, EncodableKnownSize, EncodableSync,
};

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

/// An [ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) signature suitable for Meadowcap's [`NamespaceSignature`](https://willowprotocol.org/specs/meadowcap/index.html#NamespaceSignature) and [`UserSignature`](https://willowprotocol.org/specs/meadowcap/index.html#UserSignature) parameters.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Signature25(ed25519_dalek::Signature);

impl Signature25 {
    /// Returns a reference to the inner [`ed25519_dalek::Signature`].
    pub fn inner(&self) -> &ed25519_dalek::Signature {
        &self.0
    }
}

impl Encodable for Signature25 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let slice = self.inner().to_bytes();

        consumer
            .bulk_consume_full_slice(&slice)
            .await
            .map_err(|err| err.reason)?;

        Ok(())
    }
}

impl EncodableKnownSize for Signature25 {
    fn len_of_encoding(&self) -> usize {
        64
    }
}

impl EncodableSync for Signature25 {}

impl Decodable for Signature25 {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice: [u8; 64] = [0; 64];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        // We can unwrap here because we know the slice is 64 bytes
        let sig = ed25519_dalek::Signature::from_bytes(&slice);

        Ok(Self(sig))
    }
}

impl DecodableCanonic for Signature25 {
    type ErrorCanonic = Blame;

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

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for Signature25 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes: [u8; 64] = Arbitrary::arbitrary(u)?;

        Ok(Self(ed25519_dalek::Signature::from_bytes(&bytes)))
    }
}

pub struct SigningKey25(ed25519_dalek::SigningKey);

impl SigningKey25 {
    pub(crate) fn new(key: ed25519_dalek::SigningKey) -> Self {
        Self(key)
    }
}

impl Signer<Signature25> for SigningKey25 {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature25, signature::Error> {
        self.0.try_sign(msg).map(Signature25)
    }
}
