use super::path::Path;

/// A Timestamp is a 64-bit unsigned integer, that is, a natural number between zero (inclusive) and 2^64 - 1 (exclusive).
/// Timestamps are to be interpreted as a time in microseconds since the Unix epoch.
pub type Timestamp = u64;

/// Writing of this to a store can be authorised using an [`AuthorisationToken`].
// Is the right design for Willow parameters which are functions?
pub trait WriteAuthorisable<AuthorisationToken> {
    fn is_authorised_write(&self, token: &AuthorisationToken) -> bool;
}

#[derive(Debug, PartialEq, Eq, Clone)]
// So I was wondering whether this should be a trait, but traits can't have fields/members.
/// The metadata associated with each Payload.
// const generics can only be u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, char and bool. Not flexible enough for our namespaceId, SubspaceId, etc.
pub struct Entry<NamespaceId, SubspaceId, P, PayloadDigest>
where
    NamespaceId: PartialEq + Eq,
    SubspaceId: PartialOrd + Ord + PartialEq + Eq,
    // Naming things...
    P: Path,
    PayloadDigest: PartialOrd + Ord + PartialEq + Eq,
{
    /// The identifier of the namespace to which the [`Entry`] belongs.
    pub namespace_id: NamespaceId,
    /// The identifier of the subspace to which the [`Entry`] belongs.
    pub subspace_id: SubspaceId,
    /// The [`Path`] to which the [`Entry`] was written.
    pub path: P,
    /// The claimed creation time of the [`Entry`].
    pub timestamp: Timestamp,
    /// The length of the Payload in bytes.
    pub payload_length: u64,
    /// The result of applying hash_payload to the Payload.
    pub payload_digest: PayloadDigest,
}

impl<NamespaceId, SubspaceId, P, PayloadDigest> Entry<NamespaceId, SubspaceId, P, PayloadDigest>
where
    NamespaceId: PartialEq + Eq,
    SubspaceId: PartialOrd + Ord + PartialEq + Eq,
    P: Path,
    PayloadDigest: PartialOrd + Ord + PartialEq + Eq,
{
    /// Return if this [`Entry`] is newer than another using their timestamps.
    /// Tie-breaks using the Entries' payload digest and payload length otherwise.
    pub fn is_newer_than(&self, other: &Entry<NamespaceId, SubspaceId, P, PayloadDigest>) -> bool {
        other.timestamp < self.timestamp
            || (other.timestamp == self.timestamp && other.payload_digest < self.payload_digest)
            || (other.timestamp == self.timestamp
                && other.payload_digest == self.payload_digest
                && other.payload_length < self.payload_length)
    }
}

#[derive(Debug)]
/// An error indicating an [`AuthorisationToken`] does not authorise the writing of this entry.
pub struct UnauthorisedWriteError;

/// An AuthorisedEntry is a pair of an [`Entry`] and [`AuthorisationToken`] for which [`Entry::is_authorised_write`] returns true.
pub struct AuthorisedEntry<'a, NamespaceId, SubspaceId, P, PayloadDigest, AuthorisationToken>(
    // Not sure what I'm getting myself into here.
    // Additionally, I'm being warned that these two fields are never being read?
    &'a Entry<NamespaceId, SubspaceId, P, PayloadDigest>,
    &'a AuthorisationToken,
)
where
    Entry<NamespaceId, SubspaceId, P, PayloadDigest>: WriteAuthorisable<AuthorisationToken>,
    NamespaceId: PartialEq + Eq,
    SubspaceId: PartialOrd + Ord + PartialEq + Eq,
    P: Path,
    PayloadDigest: PartialOrd + Ord + PartialEq + Eq;

impl<'a, NamespaceId, SubspaceId, P, PayloadDigest, AuthorisationToken>
    AuthorisedEntry<'a, NamespaceId, SubspaceId, P, PayloadDigest, AuthorisationToken>
where
    Entry<NamespaceId, SubspaceId, P, PayloadDigest>: WriteAuthorisable<AuthorisationToken>,
    NamespaceId: PartialEq + Eq,
    SubspaceId: PartialOrd + Ord + PartialEq + Eq,
    P: Path,
    PayloadDigest: PartialOrd + Ord + PartialEq + Eq,
{
    /// Construct an [`AuthorisedEntry`] if the token permits the writing of this entry, otherwise return a [`UnauthorisedWriteError`]
    pub fn new(
        entry: &'a Entry<NamespaceId, SubspaceId, P, PayloadDigest>,
        token: &'a AuthorisationToken,
    ) -> Result<Self, UnauthorisedWriteError> {
        if !entry.is_authorised_write(token) {
            return Err(UnauthorisedWriteError);
        }

        Ok(Self(entry, token))
    }
}
