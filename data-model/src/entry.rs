#[cfg(feature = "dev")]
use arbitrary::size_hint::and_all;
#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

use crate::{
    encoding::{DecodeError, U64BE},
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
pub struct Entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// The identifier of the namespace to which the [`Entry`] belongs.
    namespace_id: N,
    /// The identifier of the subspace to which the [`Entry`] belongs.
    subspace_id: S,
    /// The [`Path`] to which the [`Entry`] was written.
    path: Path<MCL, MCC, MPL>,
    /// The claimed creation time of the [`Entry`].
    timestamp: Timestamp,
    /// The length of the Payload in bytes.
    payload_length: u64,
    /// The result of applying hash_payload to the Payload.
    payload_digest: PD,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// Create a new [`Entry`].
    pub fn new(
        namespace_id: N,
        subspace_id: S,
        path: Path<MCL, MCC, MPL>,
        timestamp: Timestamp,
        payload_length: u64,
        payload_digest: PD,
    ) -> Self {
        Entry {
            namespace_id,
            subspace_id,
            path,
            timestamp,
            payload_length,
            payload_digest,
        }
    }

    /// Return a reference to the identifier of the namespace to which the [`Entry`] belongs.
    pub fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    /// Return a reference to the identifier of the subspace_id to which the [`Entry`] belongs.
    pub fn subspace_id(&self) -> &S {
        &self.subspace_id
    }

    /// Return a reference to the [`Path`] to which the [`Entry`] was written.
    pub fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.path
    }

    /// Return the claimed creation time of the [`Entry`].
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Return the length of the Payload in bytes.
    pub fn payload_length(&self) -> u64 {
        self.payload_length
    }

    /// Return a reference to the result of applying hash_payload to the Payload.
    pub fn payload_digest(&self) -> &PD {
        &self.payload_digest
    }

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

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    #[syncify_replace(use crate::encoding::sync::{Decodable, Encodable};)]
    use crate::encoding::{Decodable, Encodable};

    impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Encodable
        for Entry<MCL, MCC, MPL, N, S, PD>
    where
        N: NamespaceId + Encodable,
        S: SubspaceId + Encodable,
        PD: PayloadDigest + Encodable,
    {
        async fn encode<C>(&self, consumer: &mut C) -> Result<(), <C>::Error>
        where
            C: BulkConsumer<Item = u8>,
        {
            self.namespace_id.encode(consumer).await?;
            self.subspace_id.encode(consumer).await?;
            self.path.encode(consumer).await?;

            U64BE::from(self.timestamp).encode(consumer).await?;
            U64BE::from(self.payload_length).encode(consumer).await?;

            self.payload_digest.encode(consumer).await?;

            Ok(())
        }
    }

    impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Decodable
        for Entry<MCL, MCC, MPL, N, S, PD>
    where
        N: NamespaceId + Decodable,
        S: SubspaceId + Decodable,
        PD: PayloadDigest + Decodable,
    {
        async fn decode<Prod>(producer: &mut Prod) -> Result<Self, DecodeError<Prod::Error>>
        where
            Prod: BulkProducer<Item = u8>,
        {
            let namespace_id = N::decode(producer).await?;
            let subspace_id = S::decode(producer).await?;
            let path = Path::<MCL, MCC, MPL>::decode(producer).await?;
            let timestamp = U64BE::decode(producer).await?.into();
            let payload_length = U64BE::decode(producer).await?.into();
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
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Arbitrary<'a>
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId + Arbitrary<'a>,
    S: SubspaceId + Arbitrary<'a>,
    PD: PayloadDigest + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let namespace_id: N = Arbitrary::arbitrary(u)?;

        let subspace_id: S = Arbitrary::arbitrary(u)?;

        let path: Path<MCL, MCC, MPL> = Arbitrary::arbitrary(u)?;

        let payload_digest: PD = Arbitrary::arbitrary(u)?;

        Ok(Self {
            namespace_id,
            subspace_id,
            path,
            payload_digest,
            payload_length: Arbitrary::arbitrary(u)?,
            timestamp: Arbitrary::arbitrary(u)?,
        })
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        and_all(&[
            N::size_hint(depth),
            S::size_hint(depth),
            Path::<MCL, MCC, MPL>::size_hint(depth),
            PD::size_hint(depth),
            u64::size_hint(depth),
            u64::size_hint(depth),
        ])
    }
}

/// An error indicating an [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) does not authorise the writing of this entry.
#[derive(Debug)]
pub struct UnauthorisedWriteError;

/// An AuthorisedEntry is a pair of an [`Entry`] and [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) implementing [`IsAuthorisedWrite`] for which [`is_authorised_write`](https://willowprotocol.org/specs/data-model/index.html#is_authorised_write) returns true.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#AuthorisedEntry).
///
/// ## Type parameters
///
/// - `N` - The type used for [`NamespaceId`].
/// - `S` - The type used for [`SubspaceId`].
/// - `P` - The type used for [`Path`]s.
/// - `PD` - The type used for [`PayloadDigest`].
/// - `AT` - The type used for the [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken).
pub struct AuthorisedEntry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>(
    pub Entry<MCL, MCC, MPL, N, S, PD>,
    pub AT,
)
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest;

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// Construct an [`AuthorisedEntry`] if the token permits the writing of this entry, otherwise return an [`UnauthorisedWriteError`]

    pub fn new(
        entry: Entry<MCL, MCC, MPL, N, S, PD>,
        token: AT,
    ) -> Result<Self, UnauthorisedWriteError>
    where
        AT: IsAuthorisedWrite<MCL, MCC, MPL, N, S, PD>,
    {
        if token.is_authorised_write(&entry) {
            return Ok(Self(entry, token));
        }

        Err(UnauthorisedWriteError)
    }
}

#[cfg(test)]
mod tests {
    use crate::path::Component;

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
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 0,
            timestamp: 20,
        };

        let e_a2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_a1.is_newer_than(&e_a2));

        let e_b1 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest(2),
            payload_length: 0,
            timestamp: 10,
        };

        let e_b2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest(1),
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_b1.is_newer_than(&e_b2));

        let e_c1 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 2,
            timestamp: 20,
        };

        let e_c2 = Entry {
            namespace_id: FakeNamespaceId::default(),
            subspace_id: FakeSubspaceId::default(),
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: FakePayloadDigest::default(),
            payload_length: 1,
            timestamp: 20,
        };

        assert!(e_c1.is_newer_than(&e_c2));
    }
}
