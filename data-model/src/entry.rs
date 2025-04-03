#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use compact_u64::CompactU64;

use crate::{parameters::AuthorisationToken, path::Path};

/// A Timestamp is a 64-bit unsigned integer, that is, a natural number between zero (inclusive) and 2^64 (exclusive).
/// Timestamps are to be interpreted as a time in microseconds since the Unix epoch.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#Timestamp).
pub type Timestamp = u64;

/// The metadata associated with each Payload.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#Entry).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct Entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> {
    /// The identifier of the namespace to which the [`Entry`] belongs.
    namespace_id: N,
    /// The identifier of the subspace to which the [`Entry`] belongs.
    subspace_id: S,
    /// The [`Path`] to which the [`Entry`] was written.
    path: Path<MCL, MCC, MPL>,
    /// The claimed creation time of the [`Entry`].
    timestamp: Timestamp,
    /// The result of applying hash_payload to the Payload.
    payload_digest: PD,
    /// The length of the Payload in bytes.
    payload_length: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    Entry<MCL, MCC, MPL, N, S, PD>
{
    /// Creates a new [`Entry`].
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

    /// Returns a reference to the identifier of the namespace to which the [`Entry`] belongs.
    pub fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    /// Returns a reference to the identifier of the subspace_id to which the [`Entry`] belongs.
    pub fn subspace_id(&self) -> &S {
        &self.subspace_id
    }

    /// Returns a reference to the [`Path`] to which the [`Entry`] was written.
    pub fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.path
    }

    /// Returns the claimed creation time of the [`Entry`].
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Returns the length of the Payload in bytes.
    pub fn payload_length(&self) -> u64 {
        self.payload_length
    }

    /// Returns a reference to the result of applying hash_payload to the Payload.
    pub fn payload_digest(&self) -> &PD {
        &self.payload_digest
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Entry<MCL, MCC, MPL, N, S, PD>
where
    PD: PartialOrd,
{
    /// Returns if this [`Entry`] is newer than another using their timestamps.
    /// Tie-breaks using the Entries' payload digest and payload length otherwise.
    ///
    /// [Definition](https://willowprotocol.org/specs/data-model/index.html#entry_newer).
    pub fn is_newer_than(&self, other: &Self) -> bool {
        other.timestamp < self.timestamp
            || (other.timestamp == self.timestamp && other.payload_digest < self.payload_digest)
            || (other.timestamp == self.timestamp
                && other.payload_digest == self.payload_digest
                && other.payload_length < self.payload_length)
    }
}

use ufotofu::{BulkConsumer, BulkProducer};

use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodableSync, DecodeError, Encodable, EncodableKnownSize,
    EncodableSync,
};

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Encodable
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Encodable,
    S: Encodable,
    PD: Encodable,
{
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), <C>::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.namespace_id.encode(consumer).await?;
        self.subspace_id.encode(consumer).await?;
        self.path.encode(consumer).await?;

        CompactU64(self.timestamp).encode(consumer).await?;
        CompactU64(self.payload_length).encode(consumer).await?;

        self.payload_digest.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Decodable
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Decodable,
    S: Decodable,
    PD: Decodable,
    Blame: From<N::ErrorReason> + From<S::ErrorReason> + From<PD::ErrorReason>,
{
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let namespace_id = N::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let subspace_id = S::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let path = Path::<MCL, MCC, MPL>::decode(producer).await?;
        let timestamp = CompactU64::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let payload_length = CompactU64::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let payload_digest = PD::decode(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

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

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> DecodableCanonic
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: DecodableCanonic,
    S: DecodableCanonic,
    PD: DecodableCanonic,
    Blame: From<N::ErrorReason>
        + From<S::ErrorReason>
        + From<PD::ErrorReason>
        + From<N::ErrorCanonic>
        + From<S::ErrorCanonic>
        + From<PD::ErrorCanonic>,
{
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let namespace_id = N::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let subspace_id = S::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;
        let path = Path::<MCL, MCC, MPL>::decode_canonic(producer).await?;
        let timestamp = CompactU64::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let payload_length = CompactU64::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?
            .0;
        let payload_digest = PD::decode_canonic(producer)
            .await
            .map_err(DecodeError::map_other_from)?;

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

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> EncodableKnownSize
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: EncodableKnownSize,
    S: EncodableKnownSize,
    PD: EncodableKnownSize,
{
    fn len_of_encoding(&self) -> usize {
        self.namespace_id.len_of_encoding()
            + self.subspace_id.len_of_encoding()
            + self.path.len_of_encoding()
            + CompactU64(self.timestamp).len_of_encoding()
            + CompactU64(self.payload_length).len_of_encoding()
            + self.payload_digest.len_of_encoding()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> EncodableSync
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: EncodableSync,
    S: EncodableSync,
    PD: EncodableSync,
{
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> DecodableSync
    for Entry<MCL, MCC, MPL, N, S, PD>
where
    N: DecodableSync,
    S: DecodableSync,
    PD: DecodableSync,
    Blame: From<N::ErrorReason> + From<S::ErrorReason> + From<PD::ErrorReason>,
{
}

/// An error indicating an [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) does not authorise the writing of this entry.
#[derive(Debug)]
pub struct UnauthorisedWriteError;

impl core::fmt::Display for UnauthorisedWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tried to authorise the writing of an entry using an AuthorisationToken which does not permit it."
        )
    }
}

impl std::error::Error for UnauthorisedWriteError {}

/// An AuthorisedEntry is a pair of an [`Entry`] and [`AuthorisationToken`] for which [`Entry::is_authorised_write`] returns true.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#AuthorisedEntry).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct AuthorisedEntry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>(
    Entry<MCL, MCC, MPL, N, S, PD>,
    AT,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Returns an [`AuthorisedEntry`] if the token permits the writing of this entry, otherwise returns an [`UnauthorisedWriteError`].
    pub fn new(
        entry: Entry<MCL, MCC, MPL, N, S, PD>,
        token: AT,
    ) -> Result<Self, UnauthorisedWriteError>
    where
        AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
    {
        if token.is_authorised_write(&entry) {
            return Ok(Self(entry, token));
        }

        Err(UnauthorisedWriteError)
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
{
    /// Returns an [`AuthorisedEntry`] without checking if the token permits the writing of this entry.
    ///
    /// Calling this method when `token.is_authorised_write(&entry)` would return `false` is immediate undefined behaviour!
    pub unsafe fn new_unchecked(entry: Entry<MCL, MCC, MPL, N, S, PD>, token: AT) -> Self {
        Self(entry, token)
    }

    /// Splits this into [`Entry`] and [`AuthorisationToken`] halves.
    pub fn into_parts(self) -> (Entry<MCL, MCC, MPL, N, S, PD>, AT) {
        (self.0, self.1)
    }

    /// Gets a reference to the [`Entry`].
    pub fn entry(&self) -> &Entry<MCL, MCC, MPL, N, S, PD> {
        &self.0
    }

    /// Gets a reference to the [`AuthorisationToken`].
    pub fn token(&self) -> &AT {
        &self.1
    }
}

#[cfg(test)]
mod tests {
    use crate::path::Component;

    use super::*;

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn entry_newer_than() {
        let e_a1 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 0,
            payload_length: 0,
            timestamp: 20,
        };

        let e_a2 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 0,
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_a1.is_newer_than(&e_a2));

        let e_b1 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 2,
            payload_length: 0,
            timestamp: 10,
        };

        let e_b2 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 1,
            payload_length: 0,
            timestamp: 10,
        };

        assert!(e_b1.is_newer_than(&e_b2));

        let e_c1 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 0,
            payload_length: 2,
            timestamp: 20,
        };

        let e_c2 = Entry {
            namespace_id: 0,
            subspace_id: 0,
            path: Path::<MCL, MCC, MPL>::new_from_slice(&[Component::new(b"a").unwrap()]).unwrap(),
            payload_digest: 0,
            payload_length: 1,
            timestamp: 20,
        };

        assert!(e_c1.is_newer_than(&e_c2));
    }
}
