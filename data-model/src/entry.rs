use arbitrary::size_hint::and_all;
use arbitrary::Arbitrary;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        parameters::{Decoder, Encoder},
    },
    parameters::{IsAuthorisedWrite, NamespaceId, PayloadDigest, SubspaceId},
    path::Path,
};

/// A Timestamp is a 64-bit unsigned integer, that is, a natural number between zero (inclusive) and 2^64 - 1 (exclusive).
/// Timestamps are to be interpreted as a time in microseconds since the Unix epoch.
/// [Read more](https://willowprotocol.org/specs/data-model/index.html#Timestamp).
pub type Timestamp = u64;

/// The metadata associated with each Payload.
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#Entry).
///
/// ## Type parameters
///
/// - `N` - The type used for [`NamespaceId`].
/// - `S` - The type used for [`SubspaceId`].
/// - `P` - The type used for [`Path`]s.
/// - `PD` - The type used for [`PayloadDigest`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Entry<N, S, P, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest,
{
    /// The identifier of the namespace to which the [`Entry`] belongs.
    pub namespace_id: N,
    /// The identifier of the subspace to which the [`Entry`] belongs.
    pub subspace_id: S,
    /// The [`Path`] to which the [`Entry`] was written.
    pub path: P,
    /// The claimed creation time of the [`Entry`].
    pub timestamp: Timestamp,
    /// The length of the Payload in bytes.
    pub payload_length: u64,
    /// The result of applying hash_payload to the Payload.
    pub payload_digest: PD,
}

impl<N, S, P, PD> Entry<N, S, P, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest,
{
    /// Return if this [`Entry`] is newer than another using their timestamps.
    /// Tie-breaks using the Entries' payload digest and payload length otherwise.
    /// [Definition](https://willowprotocol.org/specs/data-model/index.html#entry_newer).
    pub fn is_newer_than(&self, other: &Self) -> bool {
        other.timestamp < self.timestamp
            || (other.timestamp == self.timestamp && other.payload_digest < self.payload_digest)
            || (other.timestamp == self.timestamp
                && other.payload_digest == self.payload_digest
                && other.payload_length < self.payload_length)
    }
}

impl<N, S, P, PD, C> Encoder<C> for Entry<N, S, P, PD>
where
    N: NamespaceId + Encoder<C>,
    S: SubspaceId + Encoder<C>,
    P: Path + Encoder<C>,
    PD: PayloadDigest + Encoder<C>,
    C: BulkConsumer<Item = u8>,
{
    async fn encode(&self, consumer: &mut C) -> Result<(), EncodingConsumerError<<C>::Error>> {
        self.namespace_id.encode(consumer).await?;
        self.subspace_id.encode(consumer).await?;
        self.path.encode(consumer).await?;

        consumer
            .bulk_consume_full_slice(&self.timestamp.to_be_bytes())
            .await?;

        consumer
            .bulk_consume_full_slice(&self.payload_length.to_be_bytes())
            .await?;

        self.payload_digest.encode(consumer).await?;

        Ok(())
    }
}

impl<N, S, P, PD, Prod> Decoder<Prod> for Entry<N, S, P, PD>
where
    N: NamespaceId + Decoder<Prod>,
    S: SubspaceId + Decoder<Prod>,
    P: Path + Decoder<Prod>,
    PD: PayloadDigest + Decoder<Prod>,
    Prod: BulkProducer<Item = u8>,
{
    async fn decode(producer: &mut Prod) -> Result<Self, DecodeError<Prod::Error>> {
        let namespace_id = N::decode(producer).await?;
        let subspace_id = S::decode(producer).await?;
        let path = P::decode(producer).await?;

        let mut timestamp_bytes = [0u8; 8];
        producer
            .bulk_overwrite_full_slice(&mut timestamp_bytes)
            .await?;
        let timestamp = u64::from_be_bytes(timestamp_bytes);

        let mut payload_length_bytes = [0u8; 8];
        producer
            .bulk_overwrite_full_slice(&mut payload_length_bytes)
            .await?;
        let payload_length = u64::from_be_bytes(payload_length_bytes);

        let payload_digest = PD::decode(producer).await?;

        Ok(Entry {
            namespace_id,
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        })
    }
}

impl<'a, N, S, P, PD> Arbitrary<'a> for Entry<N, S, P, PD>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
    PD: PayloadDigest + Arbitrary<'a>,
    P: Path + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            namespace_id: Arbitrary::arbitrary(u)?,
            subspace_id: Arbitrary::arbitrary(u)?,
            path: Arbitrary::arbitrary(u)?,
            payload_digest: Arbitrary::arbitrary(u)?,
            payload_length: Arbitrary::arbitrary(u)?,
            timestamp: Arbitrary::arbitrary(u)?,
        })
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        and_all(&[
            N::size_hint(depth),
            S::size_hint(depth),
            P::size_hint(depth),
            PD::size_hint(depth),
            u64::size_hint(depth),
            u64::size_hint(depth),
        ])
    }
}

/// An error indicating an [`AuthorisationToken`] does not authorise the writing of this entry.
#[derive(Debug)]
pub struct UnauthorisedWriteError;

/// An AuthorisedEntry is a pair of an [`Entry`] and [`AuthorisationToken`] for which [`Entry::is_authorised_write`] returns true.
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#AuthorisedEntry).
///
/// ## Type parameters
///
/// - `N` - The type used for [`NamespaceId`].
/// - `S` - The type used for [`SubspaceId`].
/// - `P` - The type used for [`Path`]s.
/// - `PD` - The type used for [`PayloadDigest`].
/// - `AT` - The type used for the [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken).
pub struct AuthorisedEntry<N, S, P, PD, AT>(pub Entry<N, S, P, PD>, pub AT)
where
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest;

impl<N, S, P, PD, AT> AuthorisedEntry<N, S, P, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    P: Path,
    PD: PayloadDigest,
{
    /// Construct an [`AuthorisedEntry`] if the token permits the writing of this entry, otherwise return an [`UnauthorisedWriteError`]
    pub fn new<IAW: IsAuthorisedWrite<N, S, P, PD, AT>>(
        entry: Entry<N, S, P, PD>,
        token: AT,
        is_authorised_write: IAW,
    ) -> Result<Self, UnauthorisedWriteError> {
        if is_authorised_write(&entry, &token) {
            return Ok(Self(entry, token));
        }

        Err(UnauthorisedWriteError)
    }
}

#[cfg(test)]
mod tests {
    use crate::path::{PathComponent, PathComponentBox, PathRc};

    use super::*;

    #[derive(Default, PartialEq, Eq, Clone)]
    struct FakeNamespaceId(usize);
    impl NamespaceId for FakeNamespaceId {}

    #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct FakeSubspaceId(usize);
    impl SubspaceId for FakeSubspaceId {
        fn successor(&self) -> Option<Self> {
            Some(FakeSubspaceId(self.0 + 1))
        }
    }

    #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct FakePayloadDigest(usize);
    impl PayloadDigest for FakePayloadDigest {}

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn entry_newer_than() {
        let e_a1 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 0,
            timestamp: 20,
        };

        let e_a2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_a1.is_newer_than(&e_a2));

        let e_b1 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest(2),
            payload_length: 0,
            timestamp: 10,
        };

        let e_b2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest(1),
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_b1.is_newer_than(&e_b2));

        let e_c1 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 2,
            timestamp: 20,
        };

        let e_c2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: PathRc::<MCL, MCC, MPL>::new(&[PathComponentBox::new(&[b'a']).unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 1,
            timestamp: 20,
        };

        assert!(e_c1.is_newer_than(&e_c2));
    }
}
