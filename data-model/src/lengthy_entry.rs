use crate::{AuthorisedEntry, AuthorisedEntryScheme, Entry, EntryScheme};

/// An [`Entry`] together with information about how much of its payload a given [`Store`] holds.
///
/// [Definition](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#LengthyEntry)
#[derive(Clone, PartialEq, Eq)]
pub struct LengthyEntry<const MCL: usize, const MCC: usize, const MPL: usize, Scheme: EntryScheme> {
    /// The Entry in question.
    entry: Entry<MCL, MCC, MPL, Scheme>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, Scheme: EntryScheme>
    LengthyEntry<MCL, MCC, MPL, Scheme>
{
    /// Create a new lengthy entry from a given [`Entry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: Entry<MCL, MCC, MPL, Scheme>, available: u64) -> Self {
        Self { entry, available }
    }

    /// The entry in question.
    pub fn entry(&self) -> &Entry<MCL, MCC, MPL, Scheme> {
        &self.entry
    }

    /// The number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }

    /// Turn this into a regular [`Entry`].
    pub fn into_entry(self) -> Entry<MCL, MCC, MPL, Scheme> {
        self.entry
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, Scheme: EntryScheme>
    AsRef<Entry<MCL, MCC, MPL, Scheme>> for LengthyEntry<MCL, MCC, MPL, Scheme>
{
    fn as_ref(&self) -> &Entry<MCL, MCC, MPL, Scheme> {
        &self.entry
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, Scheme: EntryScheme> std::fmt::Debug
    for LengthyEntry<MCL, MCC, MPL, Scheme>
where
    Scheme: std::fmt::Debug,
    Scheme::NamespaceId: std::fmt::Debug,
    Scheme::SubspaceId: std::fmt::Debug,
    Scheme::PayloadDigest: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&"AuthorisedEntry")
            .field("entry", self.entry())
            .field("available", &self.available)
            .finish()
    }
}

/// An [`AuthorisedEntry`] together with information about how much of its payload a given [`Store`] holds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LengthyAuthorisedEntry<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    Scheme: AuthorisedEntryScheme<MCL, MCC, MPL>,
> {
    /// The Entry in question.
    entry: AuthorisedEntry<MCL, MCC, MPL, Scheme>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        Scheme: AuthorisedEntryScheme<MCL, MCC, MPL>,
    > LengthyAuthorisedEntry<MCL, MCC, MPL, Scheme>
{
    /// Create a new lengthy entry from a given [`AuthorisedEntry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: AuthorisedEntry<MCL, MCC, MPL, Scheme>, available: u64) -> Self {
        Self { entry, available }
    }

    /// The entry in question.
    pub fn entry(&self) -> &AuthorisedEntry<MCL, MCC, MPL, Scheme> {
        &self.entry
    }

    /// The number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }

    /// Turn this into a [`AuthorisedEntry`].
    pub fn into_authorised_entry(self) -> AuthorisedEntry<MCL, MCC, MPL, Scheme> {
        self.entry
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        Scheme: AuthorisedEntryScheme<MCL, MCC, MPL>,
    > AsRef<AuthorisedEntry<MCL, MCC, MPL, Scheme>>
    for LengthyAuthorisedEntry<MCL, MCC, MPL, Scheme>
{
    fn as_ref(&self) -> &AuthorisedEntry<MCL, MCC, MPL, Scheme> {
        &self.entry
    }
}
