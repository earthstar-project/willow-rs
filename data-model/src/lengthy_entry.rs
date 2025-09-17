#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[cfg(feature = "dev")]
use crate::AuthorisationToken;
use crate::{AuthorisedEntry, Entry};

/// An [`Entry`] together with information about how much of its payload a given [`crate::Store`] holds.
///
/// [Definition](https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html#LengthyEntry)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LengthyEntry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> {
    /// The Entry in question.
    entry: Entry<MCL, MCC, MPL, N, S, PD>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    LengthyEntry<MCL, MCC, MPL, N, S, PD>
{
    /// Returns a new lengthy entry from a given [`Entry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: Entry<MCL, MCC, MPL, N, S, PD>, available: u64) -> Self {
        Self { entry, available }
    }

    /// Returns the entry in question.
    pub fn entry(&self) -> &Entry<MCL, MCC, MPL, N, S, PD> {
        &self.entry
    }

    /// Returns the number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }

    /// Turns this into a regular [`Entry`].
    pub fn into_entry(self) -> Entry<MCL, MCC, MPL, N, S, PD> {
        self.entry
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    AsRef<Entry<MCL, MCC, MPL, N, S, PD>> for LengthyEntry<MCL, MCC, MPL, N, S, PD>
{
    fn as_ref(&self) -> &Entry<MCL, MCC, MPL, N, S, PD> {
        &self.entry
    }
}

/// An [`AuthorisedEntry`] together with information about how much of its payload a given [`crate::Store`] holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LengthyAuthorisedEntry<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    N,
    S,
    PD,
    AT,
> {
    /// The Entry in question.
    entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>,
    /// The number of consecutive bytes from the start of the entry’s payload that the peer holds.
    available: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
{
    /// Returns a new lengthy entry from a given [`AuthorisedEntry`] and the number of consecutive bytes from the start of the entry’s payload that are held.
    pub fn new(entry: AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>, available: u64) -> Self {
        Self { entry, available }
    }

    /// Returns the authorised entry in question.
    pub fn entry(&self) -> &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT> {
        &self.entry
    }

    /// Returns the number of consecutive bytes from the start of the entry’s Payload that the peer holds.
    pub fn available(&self) -> u64 {
        self.available
    }

    /// Turns this into a [`AuthorisedEntry`].
    pub fn into_authorised_entry(self) -> AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT> {
        self.entry
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT>
    AsRef<AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>>
    for LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
{
    fn as_ref(&self) -> &AuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT> {
        &self.entry
    }
}

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, AT> Arbitrary<'a>
    for LengthyAuthorisedEntry<MCL, MCC, MPL, N, S, PD, AT>
where
    N: Arbitrary<'a>,
    S: Arbitrary<'a>,
    PD: Arbitrary<'a>,
    AT: AuthorisationToken<MCL, MCC, MPL, N, S, PD> + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let entry: Entry<MCL, MCC, MPL, N, S, PD> = Arbitrary::arbitrary(u)?;
        let token: AT = Arbitrary::arbitrary(u)?;

        if !token.is_authorised_write(&entry) {
            arbitrary::Result::Err(arbitrary::Error::IncorrectFormat)
        } else {
            Ok(unsafe {
                Self {
                    entry: AuthorisedEntry::new_unchecked(entry, token),
                    available: Arbitrary::arbitrary(u)?,
                }
            })
        }
    }
}
