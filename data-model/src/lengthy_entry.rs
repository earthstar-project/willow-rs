use crate::{AuthorisationToken, AuthorisedEntry, Entry, NamespaceId, PayloadDigest, SubspaceId};

/// An [`Entry`] together with information about how much of its payload a given [`Store`] holds.
///
/// [Definition](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#LengthyEntry)
pub struct LengthyEntry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// The Entry in question.
    entry: Entry<MCL, MCC, MPL, N, S, PD>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    LengthyEntry<MCL, MCC, MPL, N, S, PD>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
{
    /// Create a new lengthy entry from a given [`Entry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: Entry<MCL, MCC, MPL, N, S, PD>, available: u64) -> Self {
        Self { entry, available }
    }

    /// The entry in question.
    pub fn entry(&self) -> &Entry<MCL, MCC, MPL, N, S, PD> {
        &self.entry
    }

    /// The number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }
}

/// An [`AuthorisedEntry`] together with information about how much of its payload a given [`Store`] holds.
pub struct LengthyAuthorisedEntry<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
> where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// The Entry in question.
    entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    N: NamespaceId,
    S: SubspaceId,
    PD: PayloadDigest,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD>,
{
    /// Create a new lengthy entry from a given [`AuthorisedEntry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, available: u64) -> Self {
        Self { entry, available }
    }

    /// The entry in question.
    pub fn entry(&self) -> &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT> {
        &self.entry
    }

    /// The number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }
}
