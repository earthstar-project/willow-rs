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
