use crate::Area;
use ufotofu::{BulkConsumer, BulkProducer};

use crate::{AuthorisationToken, NamespaceId25, PayloadDigest25, SubspaceId25};

pub type CreateDropError<CE, SE> = willow_sideload::CreateDropError<CE, SE>;

pub type IngestDropError<PE, SE> = willow_sideload::IngestDropError<PE, SE>;

pub async fn create_drop<C, AreaIterator, StoreType>(
    consumer: C,
    namespace_id: NamespaceId25,
    areas: AreaIterator,
    store: &StoreType,
) -> Result<C, willow_sideload::CreateDropError<C::Error, StoreType::Error>>
where
    C: BulkConsumer<Item = u8, Final = ()>,
    StoreType: willow_sideload::SideloadStore<
        1024,
        1024,
        1024,
        NamespaceId25,
        SubspaceId25,
        PayloadDigest25,
        AuthorisationToken,
    >,
    AreaIterator: IntoIterator<Item = Area>,
{
    let encrypt = |consumer: C| consumer;
    let into_inner = |encrypted| encrypted;

    willow_sideload::create_drop(consumer, namespace_id, areas, store, encrypt, into_inner).await
}

pub async fn ingest_drop<P, StoreType>(
    producer: P,
    store: &StoreType,
) -> Result<P, willow_sideload::IngestDropError<P::Error, StoreType::Error>>
where
    P: BulkProducer<Item = u8>,
    StoreType: willow_sideload::SideloadStore<
        1024,
        1024,
        1024,
        NamespaceId25,
        SubspaceId25,
        PayloadDigest25,
        AuthorisationToken,
    >,
{
    let decrypt = |producer: P| producer;
    let into_inner = |decrypted| decrypted;

    willow_sideload::ingest_drop(producer, store, decrypt, into_inner).await
}
