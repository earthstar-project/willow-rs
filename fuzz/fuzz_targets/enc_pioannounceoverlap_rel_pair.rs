#![no_main]

use arbitrary::Arbitrary;
use ufotofu_codec::{fuzz_relative_basic, Blame, RelativeDecodable, RelativeEncodable};
use wgps::{messages::PioAnnounceOverlap, EnumerationCapability};
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakeSubspaceId};

/// An [EnumerationCapability] for use with the WGPS.

#[derive(Debug, PartialEq, Eq, Arbitrary, Clone)]
pub struct FakeEnumerationCapability {
    namespace: FakeNamespaceId,
    receiver: FakeSubspaceId,
    stuff: [u8; 8],
}

impl EnumerationCapability for FakeEnumerationCapability {
    type NamespaceId = FakeNamespaceId;
    type Receiver = FakeSubspaceId;

    fn granted_namespace(&self) -> Self::NamespaceId {
        self.namespace.clone()
    }

    fn receiver(&self) -> Self::Receiver {
        self.receiver.clone()
    }
}

impl RelativeEncodable<(FakeNamespaceId, FakeSubspaceId)> for FakeEnumerationCapability {
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &(FakeNamespaceId, FakeSubspaceId),
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer
            .bulk_consume_full_slice(&self.stuff)
            .await
            .map_err(|err| err.into_reason())?;

        Ok(())
    }
}

impl RelativeDecodable<(FakeNamespaceId, FakeSubspaceId), Blame> for FakeEnumerationCapability {
    async fn relative_decode<P>(
        producer: &mut P,
        r: &(FakeNamespaceId, FakeSubspaceId),
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut stuff = [0; 8];

        producer.bulk_overwrite_full_slice(&mut stuff).await?;

        let (namespace, receiver) = r;

        Ok(Self {
            namespace: namespace.clone(),
            receiver: receiver.clone(),
            stuff,
        })
    }
}

fuzz_relative_basic!(
    PioAnnounceOverlap<16, FakeEnumerationCapability>;
    (FakeNamespaceId, FakeSubspaceId);
    Blame;
    |msg :  &PioAnnounceOverlap<16, FakeEnumerationCapability>,  pair:  &(FakeNamespaceId, FakeSubspaceId)| {
        if let Some(ec) = &msg.enumeration_capability {
            ec.granted_namespace() == pair.0 && ec.receiver() == pair.1
        } else {
            true
        }
    }
);
