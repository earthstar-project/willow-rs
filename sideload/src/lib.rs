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

pub struct CreateDropOptions<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    FE,
    BIE,
    OE,
> {
    areas: Vec<Area<MCL, MCC, MPL, S>>,
    store: dyn Store<
        MCL,
        MCC,
        MPL,
        N,
        S,
        PD,
        AT,
        FlushError = FE,
        BulkIngestionError = BIE,
        OperationsError = OE,
    >,
}

fn create_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    FE,
    BIE,
    OE,
>(
    options: CreateDropOptions<MCL, MCC, MPL, N, S, PD, AT, FE, BIE, OE>,
) -> Result<(), CreateDropError> {
    todo!()
}
// Create a drop from a store + array of areas

// Ingest a drop into a store
