use ufotofu::{Consumer, Producer};
use ufotofu_codec::{Decodable, EncodableKnownSize, EncodableSync};
use willow_data_model::{
    grouping::Area, AuthorisationToken, NamespaceId, PayloadDigest, Store, SubspaceId,
};

pub trait SideloadNamespaceId:
    NamespaceId + EncodableSync + EncodableKnownSize + Decodable
{
    /// The least element of the set of namespace IDs.
    const DEFAULT_NAMESPACE_ID: Self;
}

pub trait SideloadSubspaceId: SubspaceId + EncodableSync + EncodableKnownSize + Decodable {
    /// The least element of the set of subspace IDs.
    const DEFAULT_SUBSPACE_ID: Self;
}

pub trait SideloadPayloadDigest:
    PayloadDigest + EncodableSync + EncodableKnownSize + Decodable
{
    /// The least element of the set of payload digests.
    const DEFAULT_PAYLOAD_DIGEST: Self;
}

pub trait SideloadAuthorisationToken<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
>:
    AuthorisationToken<MCL, MCC, MPL, N, S, PD> + EncodableSync + EncodableKnownSize + Decodable
{
}

pub enum CreateDropError {
    EmptyDrop,
}

fn create_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    C,
    EncryptedC,
    EncryptFn,
    StoreType,
    AreaIterator,
>(
    consumer: C,
    encrypt: EncryptFn,
    areas: AreaIterator,
    store: &StoreType,
) -> Result<(), CreateDropError>
where
    C: Consumer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    AreaIterator: IntoIterator<Item = Area<MCL, MCC, MPL, S>>,
    EncryptFn: Fn(C) -> EncryptedC,
{
    // https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol
    todo!()
}

fn ingest_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    P,
    DecryptedP,
    DecryptFn,
    StoreType,
    AreaIterator,
>(
    producer: P,
    store: &StoreType,
) -> Result<(), CreateDropError>
where
    P: Producer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    DecryptFn: Fn(P) -> DecryptedP,
{
    // do the inverse of https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol
    todo!()
}
