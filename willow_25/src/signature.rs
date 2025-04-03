use ufotofu_codec::{Blame, Decodable, Encodable, EncodableKnownSize, EncodableSync};

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Signature25(ed25519_dalek::Signature);

impl Signature25 {
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

        producer.bulk_overwrite_full_slice(&mut slice).await;

        // We can unwrap here because we know the slice is 64 bytes
        let sig = ed25519_dalek::Signature::from_bytes(&slice);

        Ok(Self(sig))
    }
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for Signature25 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes: [u8; 64] = Arbitrary::arbitrary(u)?;

        Ok(Self(ed25519_dalek::Signature::from_bytes(&bytes)))
    }
}
