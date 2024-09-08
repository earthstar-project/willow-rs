use crate::{AuthorisationToken, NamespaceId, PayloadDigest, SubspaceId};

pub trait EntryScheme: PartialEq + Eq {
    type NamespaceId: NamespaceId;
    type SubspaceId: SubspaceId;
    type PayloadDigest: PayloadDigest;
}

pub trait AuthorisedEntryScheme<const MCL: usize, const MCC: usize, const MPL: usize>:
    PartialEq + Eq
{
    type Entry: EntryScheme;
    type AuthorisationToken: AuthorisationToken<MCL, MCC, MPL, Self::Entry>;
}
