#![no_main]

use arbitrary::Arbitrary;
use ufotofu_codec::{fuzz_relative_basic, Blame, Encodable, RelativeDecodable, RelativeEncodable};
use wgps::{messages::PioBindReadCapability, ReadCapability};
use willow_data_model::grouping::Area;
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakeSubspaceId};
use willow_pio::PersonalPrivateInterest;

#[derive(Debug, PartialEq, Eq, Arbitrary, Clone)]
pub struct FakeReadCapability {
    namespace: FakeNamespaceId,
    receiver: FakeSubspaceId,
    area: Area<16, 16, 16, FakeSubspaceId>,
}

impl ReadCapability<16, 16, 16> for FakeReadCapability {
    type NamespaceId = FakeNamespaceId;
    type Receiver = FakeSubspaceId;
    type SubspaceId = FakeSubspaceId;

    fn granted_namespace(&self) -> Self::NamespaceId {
        self.namespace.clone()
    }

    fn granted_area(&self) -> willow_data_model::grouping::Area<16, 16, 16, Self::SubspaceId> {
        self.area.clone()
    }
}

impl RelativeEncodable<PersonalPrivateInterest<16, 16, 16, FakeNamespaceId, FakeSubspaceId>>
    for FakeReadCapability
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &PersonalPrivateInterest<16, 16, 16, FakeNamespaceId, FakeSubspaceId>,
    ) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let base_area = Area::new_full();
        self.area.relative_encode(consumer, &base_area).await?;

        Ok(())
    }
}

impl RelativeDecodable<PersonalPrivateInterest<16, 16, 16, FakeNamespaceId, FakeSubspaceId>, Blame>
    for FakeReadCapability
{
    async fn relative_decode<P>(
        producer: &mut P,
        r: &PersonalPrivateInterest<16, 16, 16, FakeNamespaceId, FakeSubspaceId>,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let base_area = Area::new_full();
        let area =
            Area::<16, 16, 16, FakeSubspaceId>::relative_decode(producer, &base_area).await?;

        Ok(Self {
            namespace: r.private_interest().namespace_id().clone(),
            receiver: r.user_key().clone(),
            area,
        })
    }
}

fuzz_relative_basic!(
    PioBindReadCapability<16, 16, 16, FakeReadCapability>;
    PersonalPrivateInterest<16,16, 16, FakeNamespaceId, FakeSubspaceId>;
    Blame;
    |msg :  &PioBindReadCapability<16, 16, 16, FakeReadCapability>,  ppi:  &PersonalPrivateInterest<16,16, 16, FakeNamespaceId, FakeSubspaceId>| {
       &msg.capability.granted_namespace() == ppi.private_interest().namespace_id() && ppi.private_interest().includes_area(&msg.capability.granted_area()) && &msg.capability.receiver == ppi.user_key()
    }
);
