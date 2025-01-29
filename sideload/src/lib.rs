use either::Either;
use ufotofu::{BulkConsumer, Consumer, Producer};
use ufotofu_codec::{Decodable, Encodable, EncodableKnownSize, EncodableSync, RelativeEncodable};
use ufotofu_codec_endian::U64BE;
use willow_data_model::{
    grouping::{Area, AreaOfInterest},
    AuthorisationToken, Entry, NamespaceId, Path, PayloadDigest, QueryIgnoreParams, QueryOrder,
    Store, SubspaceId,
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

pub enum CreateDropError<ConsumerError> {
    EmptyDrop,
    StoreErr,
    ConsumerProblem(ConsumerError),
}

async fn create_drop<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N: SideloadNamespaceId,
    S: SideloadSubspaceId,
    PD: SideloadPayloadDigest,
    AT: SideloadAuthorisationToken<MCL, MCC, MPL, N, S, PD>,
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
) -> Result<(), CreateDropError<EncryptedC::Error>>
where
    C: BulkConsumer<Item = u8>,
    EncryptedC: BulkConsumer<Item = u8>,
    StoreType: Store<MCL, MCC, MPL, N, S, PD, AT>,
    AreaIterator: IntoIterator<Item = Area<MCL, MCC, MPL, S>>,
    EncryptFn: Fn(C) -> EncryptedC,
{
    // https://willowprotocol.org/specs/sideloading/index.html#sideload_protocol

    let mut encrypted_consumer = encrypt(consumer);

    let mut entries_count = 0;

    let mut next_areas_vec: Vec<Area<MCL, MCC, MPL, S>> = Vec::new();

    for area in areas {
        entries_count += store.count_area(&area.clone().into()).await;

        next_areas_vec.push(area);
    }

    U64BE(entries_count)
        .encode(&mut encrypted_consumer)
        .await
        .map_err(|err| CreateDropError::ConsumerProblem(err))?;

    let mut entry_to_encode_against = Entry::new(
        N::default(),
        S::default(),
        Path::<MCL, MCC, MPL>::new_empty(),
        0,
        0,
        PD::default(),
    );

    for area in next_areas_vec {
        let aoi: AreaOfInterest<MCL, MCC, MPL, S> = area.into();

        let mut entry_producer = store.query_area(
            &aoi,
            &QueryOrder::Subspace,
            false,
            Some(QueryIgnoreParams {
                ignore_incomplete_payloads: true,
                ignore_empty_payloads: true,
            }),
        );

        loop {
            match entry_producer.produce().await {
                Ok(Either::Left(lengthy)) => {
                    let authed_entry = lengthy.entry();

                    // Encode entry
                    let entry = authed_entry.entry();
                    entry
                        .relative_encode(&mut encrypted_consumer, &entry_to_encode_against)
                        .await
                        .map_err(|err| CreateDropError::ConsumerProblem(err));

                    entry_to_encode_against = entry.clone();

                    // Encode token
                    let token = authed_entry.token();
                    token
                        .encode(&mut encrypted_consumer)
                        .await
                        .map_err(|err| CreateDropError::ConsumerProblem(err));

                    // Consume payload
                    todo!("Pipe the payload bytes")
                }
                Ok(Either::Right(_)) => {
                    break;
                }
                Err(_) => return Err(CreateDropError::StoreErr),
            };
        }
    }

    Ok(())
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
