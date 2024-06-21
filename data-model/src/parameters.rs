use super::{
    entry::{AuthorisedEntry, Entry, UnauthorisedWriteError},
    path::Path,
};

/// A type for identifying [namespaces](https://willowprotocol.org/specs/data-model/index.html#namespace).
/// [Read more](https://willowprotocol.org/specs/data-model/index.html#NamespaceId).
pub trait NamespaceId: PartialEq + Eq {
    /// A [`NamespaceId`] for when we have to provide one but its meaning doesn't matter.
    const DEFAULT_NAMESPACE_ID: Self;
}

/// A type SubspaceId for identifying [subspaces](https://willowprotocol.org/specs/data-model/index.html#subspace).
/// [Read more](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).
pub trait SubspaceId: PartialOrd + Ord + PartialEq + Eq {
    /// A [`SubspaceId`] for when we have to provide one but its meaning doesn't matter.
    const DEFAULT_SUBSPACE_ID: Self;
}

/// A totally ordered type for [content-addressing](https://en.wikipedia.org/wiki/Content_addressing) the data that Willow stores.
/// [Read more](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest).
pub trait PayloadDigest: PartialOrd + Ord + PartialEq + Eq {
    /// A [`PayloadDigest`] for when we have to provide one but its meaning doesn't matter.
    const DEFAULT_PAYLOAD_DIGEST: Self;
}

/// A function that maps an [`Entry`] and an [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) to a result indicating whether the `AuthorisationToken` does prove write permission for the [`Entry`].
/// [Read more](https://willowprotocol.org/specs/data-model/index.html#is_authorised_write).
///
/// This function intentionally deviates from the specification's definition (in which `is_authorised_write` returns a `bool`) so that the result of the function cannot be accidentally ignored.
///
/// ## Type parameters
///
/// - `N` - The type used for [`NamespaceId`].
/// - `S` - The type used for [`SubspaceId`].
/// - `P` - The type used for [`Path`]s.
/// - `PD` - The type used for [`PayloadDigest`].
/// - `AT` - The type used for the [`AuthorisationToken` (willowprotocol.org)](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken).
pub trait IsAuthorisedWrite<N: NamespaceId, S: SubspaceId, P: Path, PD: PayloadDigest, AT>:
    Fn(Entry<N, S, P, PD>, AT) -> Result<AuthorisedEntry<N, S, P, PD, AT>, UnauthorisedWriteError>
{
}