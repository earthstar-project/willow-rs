use super::{
    entry::{AuthorisedEntry, Entry, UnauthorisedWriteError},
    path::Path,
};

/// A type for identifying [namespaces](https://willowprotocol.org/specs/data-model/index.html#namespace).
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#NamespaceId).

pub trait NamespaceId: Eq + Default {}

/// A type for identifying [subspaces](https://willowprotocol.org/specs/data-model/index.html#subspace).
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).
///
/// ## Implementation notes
///
/// The [`Default`] implementation **must** return the least element in the total order of [`SubspaceId`].
pub trait SubspaceId: Ord + Default {}

/// A totally ordered type for [content-addressing](https://en.wikipedia.org/wiki/Content_addressing) the data that Willow stores.
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest).
pub trait PayloadDigest: Ord + Default {}

/// A function that maps an [`Entry`] and an [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) to a result indicating whether the `AuthorisationToken` does prove write permission for the [`Entry`].
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#is_authorised_write).
///
/// ## Type parameters
///
/// - `N` - The type used for [`NamespaceId`].
/// - `S` - The type used for [`SubspaceId`].
/// - `P` - The type used for [`Path`]s.
/// - `PD` - The type used for [`PayloadDigest`].
/// - `AT` - The type used for the [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken).
pub trait IsAuthorisedWrite<N: NamespaceId, S: SubspaceId, P: Path, PD: PayloadDigest, AT>:
    Fn(&Entry<N, S, P, PD>, &AT) -> bool
{
}
