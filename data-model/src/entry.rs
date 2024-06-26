use super::{
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
    impl SubspaceId for FakeSubspaceId {}

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
