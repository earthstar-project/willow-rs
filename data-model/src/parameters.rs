use crate::{entry::Entry, EntryScheme};

/// A type for identifying [namespaces](https://willowprotocol.org/specs/data-model/index.html#namespace).
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#NamespaceId).

pub trait NamespaceId: Eq + Default + Clone {}

/// A type for identifying [subspaces](https://willowprotocol.org/specs/data-model/index.html#subspace).
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).
///
/// ## Implementation notes
///
/// The [`Default`] implementation **must** return the least element in the total order of [`SubspaceId`].
pub trait SubspaceId: Ord + Default + Clone {
    /// Return the next possible value in the set of all [`SubspaceId`].
    /// e.g. the successor of 3 is 4.
    fn successor(&self) -> Option<Self>;
}

/// A totally ordered type for [content-addressing](https://en.wikipedia.org/wiki/Content_addressing) the data that Willow stores.
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest).
pub trait PayloadDigest: Ord + Default + Clone {}

/// Determines whether this type (nominally a [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken)) is able to prove write permission for a given [`Entry`].
///
/// ## Type parameters
///
/// - `N` - The type used for the [`Entry`]'s [`NamespaceId`].
/// - `S` - The type used for the [`Entry`]'s [`SubspaceId`].
/// - `PD` - The type used for the [`Entry`]'s [`PayloadDigest`].
pub trait AuthorisationToken<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    Scheme: EntryScheme,
>
{
    /// Determine whether this type (nominally a [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken)) is able to prove write permission for a given [`Entry`].
    fn is_authorised_write(&self, entry: &Entry<MCL, MCC, MPL, Scheme>) -> bool;
}
