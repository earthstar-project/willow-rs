use crate::grouping::Range;
use crate::{entry::Entry, grouping::RangeEnd};
use core::fmt::Debug;

/// A type for identifying [namespaces](https://willowprotocol.org/specs/data-model/index.html#namespace).
///
/// When an implementing type implements [`PartialOrd`], then `Self::default()` must return a least element with respect to the ordering.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#NamespaceId).
pub trait NamespaceId: Eq + Default + Clone + Debug {}

/// A type for identifying [subspaces](https://willowprotocol.org/specs/data-model/index.html#subspace).
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#SubspaceId).
///
/// ## Implementation notes
///
/// The [`Default`] implementation **must** return the least element in the total order of [`SubspaceId`].
pub trait SubspaceId: Ord + Default + Clone + Debug {
    /// Returns the next possible value in the set of all [`SubspaceId`], i.e. the (unique) least value that is strictly greater than `self`. Only if there is no greater value at all may this method return `None`.
    fn successor(&self) -> Option<Self>;

    /// Returns a range which *only* [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) the value of `self`.
    fn singleton_range(&self) -> Range<Self> {
        match self.successor() {
            Some(successor) => Range {
                start: self.clone(),
                end: RangeEnd::Closed(successor),
            },
            None => Range::new_open(self.clone()),
        }
    }
}

/// A totally ordered type for [content-addressing](https://en.wikipedia.org/wiki/Content_addressing) the data that Willow stores.
///
/// This trait mirrors the [`std::hash::Hasher`] trait (with the hasher state as an associated type), but hashing not to `u64`s but to `Self`s.
///
/// [Definition](https://willowprotocol.org/specs/data-model/index.html#PayloadDigest).
///
/// ## Implementation notes
///
/// The [`Default`] implementation **must** return the least element in the total order of [`PayloadDigest`].
pub trait PayloadDigest: Ord + Default + Clone + Debug {
    /// The state for hashing slices of bytes.
    type Hasher;

    /// Returns the initial state for hashing, i.e., the hash for the empty string.
    fn hasher() -> Self::Hasher;

    /// Returns the digest for the values written so far.
    ///
    /// Despite its name, the method does not reset the hasher’s internal state. Additional writes will continue from the current value. If you need to start a fresh hash value, you will have to create a new hasher.
    fn finish(hasher: &Self::Hasher) -> Self;

    /// Writes some data into the given Hasher.
    fn write(hasher: &mut Self::Hasher, bytes: &[u8]);
}

/// Determines whether this type (nominally an [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken)) is able to prove write permission for a given [`Entry`].
///
/// ## Type parameters
///
/// - `N` - The type used for the [`Entry`]'s [`NamespaceId`].
/// - `S` - The type used for the [`Entry`]'s [`SubspaceId`].
/// - `PD` - The type used for the [`Entry`]'s [`PayloadDigest`].
pub trait AuthorisationToken<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>:
    Clone + Debug
{
    /// Determines whether this type (nominally a [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken)) is able to prove write permission for a given [`Entry`].
    fn is_authorised_write(&self, entry: &Entry<MCL, MCC, MPL, N, S, PD>) -> bool;
}
