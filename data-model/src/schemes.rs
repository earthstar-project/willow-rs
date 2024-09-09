use std::fmt::Debug;

use crate::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId};

pub trait EntryScheme: Debug + PartialEq + Eq + Clone {
    type NamespaceId: NamespaceId;
    type SubspaceId: SubspaceId;
    type PayloadDigest: PayloadDigest;
}

pub trait AuthorisedEntryScheme<const MCL: usize, const MCC: usize, const MPL: usize>:
    PartialEq + Eq + Debug + Clone
{
    type Entry: EntryScheme;
    type AuthorisationToken: AuthorisationToken<MCL, MCC, MPL, Self::Entry>;
}
