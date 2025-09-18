use crate::{Area, Capability, Entry, Path, SigningKey25};
use meadowcap::AccessMode;
use ufotofu::{BulkConsumer, BulkProducer};
use willow_sideload::{SideloadNamespaceId, SideloadPayloadDigest, SideloadSubspaceId};

use crate::{AuthorisationToken, NamespaceId25, PayloadDigest25, SubspaceId25};

pub type CreateDropError<CE, SE> = willow_sideload::CreateDropError<CE, SE>;

pub type IngestDropError<PE, SE> = willow_sideload::IngestDropError<PE, SE>;

pub async fn create_drop<C, AreaIterator, StoreType>(
    consumer: C,
    namespace_id: NamespaceId25,
    areas: AreaIterator,
    store: std::rc::Rc<StoreType>,
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

    let default_entry = Entry::new(
        NamespaceId25::default_namespace_id(),
        SubspaceId25::default_subspace_id(),
        Path::new_empty(),
        0,
        0,
        PayloadDigest25::default_payload_digest(),
    );

    let default_cap = Capability::new_communal(
        NamespaceId25::default_namespace_id(),
        SubspaceId25::default_subspace_id(),
        AccessMode::Write,
    )
    .unwrap();

    let default_token = default_cap
        .authorisation_token(&default_entry, SigningKey25::default())
        .unwrap();

    willow_sideload::create_drop(
        consumer,
        namespace_id,
        areas,
        store,
        encrypt,
        into_inner,
        default_token,
    )
    .await
}

pub async fn ingest_drop<P, StoreType>(
    producer: P,
    store: &StoreType,
) -> Result<P, willow_sideload::IngestDropError<P::Error, StoreType::Error>>
where
    P: BulkProducer<Item = u8> + std::fmt::Debug,
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

    let default_entry = Entry::new(
        NamespaceId25::default_namespace_id(),
        SubspaceId25::default_subspace_id(),
        Path::new_empty(),
        0,
        0,
        PayloadDigest25::default_payload_digest(),
    );

    let default_cap = Capability::new_communal(
        NamespaceId25::default_namespace_id(),
        SubspaceId25::default_subspace_id(),
        AccessMode::Write,
    )
    .unwrap();

    let default_token = default_cap
        .authorisation_token(&default_entry, SigningKey25::default())
        .unwrap();

    willow_sideload::ingest_drop(producer, store, decrypt, into_inner, default_token).await
}
