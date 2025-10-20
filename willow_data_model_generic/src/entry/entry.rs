use std::cmp::Ordering;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

use derive_builder::Builder;

use crate::prelude::*;

/// The metadata associated with each Willow Payload string.
///
/// Entries are the central concept in Willow. In order to make any bytestring of data accessible to Willow, you need to create an Entry describing its metadata. Specifically, an Entry consists of
///
/// - a *namespace id* (roughly, this addresses a universe of Willow data, fully independent from all data (i.e., Entries) of different namespace ids) of type `N`,
/// - a *subspace id* (roughly, a fully indendent part of a namespace, typically subspaces correspond to individual users) of type `S`,
/// - a *path* (roughly, a file-system-like way of arranging payloads hierarchically within a subspace) of type [`Path`],
/// - a *timestamp* (newer Entries can overwrite certain older Entries),
/// - a *payload digest* (a secure hash of the payload string being inserted into Willow), and
/// - a *payload length* (the length of the payload string).
///
/// For precise information about these six fields of an Entry, see the [specification](https://willowprotocol.org/specs/data-model/index.html#Entry) — it is quite readable.
///
/// T access these six fields, use the methods of the [`Entrylike`] trait (which [`Entry`] implements). The [`EntrylikeExt`] trait provides additional helper methods, for example, methods to check which Entries can delete which other Entries.
///
/// To create Entries, use the [`Entry::builder`] or [`Entry::prefilled_builder`] functions.
///
/// # Example
///
/// ```
/// use willow_data_model_generic::prelude::*;
///
/// let entry = Entry::builder()
///     .namespace_id("family")
///     .subspace_id("alfie")
///     .path(Path::<4, 4, 4>::new())
///     .timestamp(12345)
///     .payload_digest("some_hash")
///     .payload_length(17)
///     .build().unwrap();
///
/// assert_eq!(*entry.subspace_id(), "alfie");
///
/// let newer = Entry::prefilled_builder(&entry).timestamp(99999).build().unwrap();
/// assert!(newer.prunes(&entry));
/// ```
///
/// [Spec definition](https://willowprotocol.org/specs/data-model/index.html#Entry).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Builder)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct Entry<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> {
    /// The identifier of the namespace to which the [`Entry`] belongs.
    namespace_id: N,
    /// The identifier of the subspace to which the [`Entry`] belongs.
    subspace_id: S,
    /// The [`Path`] to which the [`Entry`] was written.
    path: Path<MCL, MCC, MPL>,
    /// The claimed creation time of the [`Entry`].
    timestamp: Timestamp,
    /// The result of applying hash_payload to the Payload.
    payload_digest: PD,
    /// The length of the Payload in bytes.
    payload_length: u64,
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Entry<MCL, MCC, MPL, N, S, PD>
where
    N: Clone,
    S: Clone,
    PD: Clone,
{
    /// Creates a builder for [`Entry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    ///
    /// // Supplying incomplete data errors.
    /// assert!(
    ///     Entry::builder()
    ///     .path(Path::<4, 4, 4>::new())
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .payload_digest("some_hash")
    ///     // timestamp and payload_length are missing!
    ///     .build().is_err()
    /// );
    ///
    /// // Supplying all necessary data yields an entry.
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("some_hash")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert_eq!(*entry.subspace_id(), "alfie");
    /// ```
    pub fn builder() -> EntryBuilder<MCL, MCC, MPL, N, S, PD> {
        EntryBuilder::create_empty()
    }

    /// Creates a builder which is prefilled with the data from some other entry.
    ///
    /// Use this function to create modified copies of entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    ///
    /// // Supplying all necessary data yields an entry.
    /// let first_entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("some_hash")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert_eq!(*first_entry.payload_digest(), "some_hash");
    ///
    /// let second_entry = Entry::prefilled_builder(&first_entry)
    ///     .timestamp(67890)
    ///     .payload_digest("another_hash")
    ///     .payload_length(4)
    ///     .build().unwrap();
    ///
    /// assert_eq!(*second_entry.payload_digest(), "another_hash");
    /// ```
    pub fn prefilled_builder<E>(source: &E) -> EntryBuilder<MCL, MCC, MPL, N, S, PD>
    where
        E: Entrylike<MCL, MCC, MPL, N, S, PD>,
        N: Clone,
        S: Clone,
        PD: Clone,
    {
        let mut builder = Self::builder();

        builder
            .namespace_id(source.namespace_id().clone())
            .subspace_id(source.subspace_id().clone())
            .path(source.path().clone())
            .timestamp(source.timestamp())
            .payload_digest(source.payload_digest().clone())
            .payload_length(source.payload_length());

        builder
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD> Keylike<MCL, MCC, MPL, S>
    for Entry<MCL, MCC, MPL, N, S, PD>
{
    fn subspace_id(&self) -> &S {
        &self.subspace_id
    }

    fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.path
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>
    Entrylike<MCL, MCC, MPL, N, S, PD> for Entry<MCL, MCC, MPL, N, S, PD>
{
    fn namespace_id(&self) -> &N {
        &self.namespace_id
    }

    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    fn payload_length(&self) -> u64 {
        self.payload_length
    }

    fn payload_digest(&self) -> &PD {
        &self.payload_digest
    }
}

/// An entrylike value is one that provides at least as much information as an [Entry](https://willowprotocol.org/specs/data-model/index.html#Entry) provides.
pub trait Entrylike<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>:
    Keylike<MCL, MCC, MPL, S>
{
    /// Returns the NamespaceId of `self`.
    fn namespace_id(&self) -> &N;

    /// Returns the Timestamp of `self`.
    fn timestamp(&self) -> Timestamp;

    /// Returns the payload length of `self`.
    fn payload_length(&self) -> u64;

    /// Returns the PayloadDigest of `self`.
    fn payload_digest(&self) -> &PD;
}

/// Functions for working with [`Entrylikes`](Entrylike).
pub trait EntrylikeExt<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD>:
    Entrylike<MCL, MCC, MPL, N, S, PD>
{
    /// Returns whether `self` and `other` describe equal entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert!(entry.entry_eq(&entry));
    ///
    /// let changed = Entry::prefilled_builder(&entry).timestamp(999999).build().unwrap();
    /// assert!(!entry.entry_eq(&changed));
    ///
    /// ```
    fn entry_eq<OtherEntry>(&self, other: &OtherEntry) -> bool
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        N: PartialEq,
        S: PartialEq,
        PD: PartialEq,
    {
        return self.namespace_id() == other.namespace_id()
            && self.subspace_id() == other.subspace_id()
            && self.path() == other.path()
            && self.timestamp() == other.timestamp()
            && self.payload_digest() == other.payload_digest()
            && self.payload_length() == other.payload_length();
    }

    /// Compares `self` to another entry by timestamp, payload_digest second (in case of a tie), and payload_length third (in case of yet another tie). See also [`EntrylikeExt::is_newer_than`] and [`EntrylikeExt::is_older_than`].
    ///
    /// Comparing recency is primarily important to determine [which entries overwrite each other](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning); the [`EntrylikeExt::prunes`] method checks for that directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::cmp::Ordering;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert_eq!(entry.cmp_recency(&entry), Ordering::Equal);
    ///
    /// let lesser_timestamp = Entry::prefilled_builder(&entry).timestamp(5).build().unwrap();
    /// assert_eq!(entry.cmp_recency(&lesser_timestamp), Ordering::Greater);
    ///
    /// let lesser_digest = Entry::prefilled_builder(&entry).payload_digest("a").build().unwrap();
    /// assert_eq!(entry.cmp_recency(&lesser_digest), Ordering::Greater);
    ///
    /// let lesser_length = Entry::prefilled_builder(&entry).payload_length(0).build().unwrap();
    /// assert_eq!(entry.cmp_recency(&lesser_length), Ordering::Greater);
    ///
    /// let greater_timestamp = Entry::prefilled_builder(&entry).timestamp(999999).build().unwrap();
    /// assert_eq!(entry.cmp_recency(&greater_timestamp), Ordering::Less);
    ///
    /// let greater_digest = Entry::prefilled_builder(&entry).payload_digest("c").build().unwrap();
    /// assert_eq!(entry.cmp_recency(&greater_digest), Ordering::Less);
    ///
    /// let greater_length = Entry::prefilled_builder(&entry).payload_length(99).build().unwrap();
    /// assert_eq!(entry.cmp_recency(&greater_length), Ordering::Less);
    /// ```
    ///
    /// [Spec definition](https://willowprotocol.org/specs/data-model/index.html#entry_newer).
    fn cmp_recency<OtherEntry>(&self, other: &OtherEntry) -> Ordering
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        PD: Ord,
    {
        self.timestamp().cmp(&other.timestamp()).then_with(|| {
            self.payload_digest()
                .cmp(other.payload_digest())
                .then_with(|| self.payload_length().cmp(&other.payload_length()))
        })
    }

    /// Returns whether this entry is strictly [newer](https://willowprotocol.org/specs/data-model/index.html#entry_newer) than another entry. See also [`EntrylikeExt::cmp_recency`] and [`EntrylikeExt::is_older_than`].
    ///
    /// Comparing recency is primarily important to determine [which entries overwrite each other](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning); the [`EntrylikeExt::prunes`] method checks for that directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert!(!entry.is_newer_than(&entry));
    ///
    /// let lesser_timestamp = Entry::prefilled_builder(&entry).timestamp(5).build().unwrap();
    /// assert!(entry.is_newer_than(&lesser_timestamp));
    ///
    /// let lesser_digest = Entry::prefilled_builder(&entry).payload_digest("a").build().unwrap();
    /// assert!(entry.is_newer_than(&lesser_digest));
    ///
    /// let lesser_length = Entry::prefilled_builder(&entry).payload_length(0).build().unwrap();
    /// assert!(entry.is_newer_than(&lesser_length));
    ///
    /// let greater_timestamp = Entry::prefilled_builder(&entry).timestamp(999999).build().unwrap();
    /// assert!(!entry.is_newer_than(&greater_timestamp));
    ///
    /// let greater_digest = Entry::prefilled_builder(&entry).payload_digest("c").build().unwrap();
    /// assert!(!entry.is_newer_than(&greater_digest));
    ///
    /// let greater_length = Entry::prefilled_builder(&entry).payload_length(99).build().unwrap();
    /// assert!(!entry.is_newer_than(&greater_length));
    /// ```
    fn is_newer_than<OtherEntry>(&self, other: &OtherEntry) -> bool
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        PD: Ord,
    {
        self.cmp_recency(other) == Ordering::Greater
    }

    /// Returns whether this entry is strictly [older](https://willowprotocol.org/specs/data-model/index.html#entry_newer) than another entry. See also [`EntrylikeExt::cmp_recency`] and [`EntrylikeExt::is_newer_than`].
    ///
    /// Comparing recency is primarily important to determine [which entries overwrite each other](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning); the [`EntrylikeExt::prunes`] method checks for that directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::new())
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// assert!(!entry.is_older_than(&entry));
    ///
    /// let lesser_timestamp = Entry::prefilled_builder(&entry).timestamp(5).build().unwrap();
    /// assert!(!entry.is_older_than(&lesser_timestamp));
    ///
    /// let lesser_digest = Entry::prefilled_builder(&entry).payload_digest("a").build().unwrap();
    /// assert!(!entry.is_older_than(&lesser_digest));
    ///
    /// let lesser_length = Entry::prefilled_builder(&entry).payload_length(0).build().unwrap();
    /// assert!(!entry.is_older_than(&lesser_length));
    ///
    /// let greater_timestamp = Entry::prefilled_builder(&entry).timestamp(999999).build().unwrap();
    /// assert!(entry.is_older_than(&greater_timestamp));
    ///
    /// let greater_digest = Entry::prefilled_builder(&entry).payload_digest("c").build().unwrap();
    /// assert!(entry.is_older_than(&greater_digest));
    ///
    /// let greater_length = Entry::prefilled_builder(&entry).payload_length(99).build().unwrap();
    /// assert!(entry.is_older_than(&greater_length));
    /// ```
    fn is_older_than<OtherEntry>(&self, other: &OtherEntry) -> bool
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        PD: Ord,
    {
        self.cmp_recency(other) == Ordering::Less
    }

    /// Returns whether this entry would [prefix prune](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning) another entry.
    ///
    /// Prefix pruning powers deletion in Willow: whenever a data store would contain two entries, one of which pruens the other, the other is removed from the data store (or never inserted in the first place). Informally speaking, newer entries remove older entries, but only if they are in the same namespace and subspace, and only if the path of the newer entry is a prefix of the path of the older entry.
    ///
    /// More precisely: an entry `e1` prunes an entry `e2` if and only if
    ///
    /// - `e1.namespace_id() == e2.namespace_id()`,
    /// - `e1.subspace_id() == e2.subspace_id()`,
    /// - `e1.path().is_prefix_of(e2.path())`, and
    /// - `e1.is_newer_than(&e2)`.
    ///
    /// This method is the reciprocal of [`EntrylikeExt::is_pruned_by`].
    ///
    /// # Examples
    ///
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::from_slices(&["a", "b"])?)
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// let newer = Entry::prefilled_builder(&entry).timestamp(99999).build().unwrap();
    /// assert!(!entry.prunes(&newer));
    /// assert!(newer.prunes(&entry));
    ///
    /// let newer_and_prefix = Entry::prefilled_builder(&newer)
    ///     .path(Path::<4, 4, 4>::from_slices(&["a"])?).build().unwrap();
    /// assert!(!entry.prunes(&newer_and_prefix));
    /// assert!(newer_and_prefix.prunes(&entry));
    ///
    /// let newer_and_extension = Entry::prefilled_builder(&newer)
    ///     .path(Path::<4, 4, 4>::from_slices(&["a", "b", "c"])?).build().unwrap();
    /// assert!(!entry.prunes(&newer_and_extension));
    /// assert!(!newer_and_extension.prunes(&entry));
    ///
    /// let newer_but_unrelated_namespace = Entry::prefilled_builder(&newer)
    ///     .namespace_id("bookclub").build().unwrap();
    /// assert!(!entry.prunes(&newer_but_unrelated_namespace));
    /// assert!(!newer_but_unrelated_namespace.prunes(&entry));
    ///
    /// let newer_but_unrelated_subspace = Entry::prefilled_builder(&newer)
    ///     .subspace_id("betty").build().unwrap();
    /// assert!(!entry.prunes(&newer_but_unrelated_subspace));
    /// assert!(!newer_but_unrelated_subspace.prunes(&entry));
    /// # Ok::<(), PathError>(())
    /// ```
    ///
    fn prunes<OtherEntry>(&self, other: &OtherEntry) -> bool
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        N: PartialEq,
        S: PartialEq,
        PD: Ord,
    {
        self.is_newer_than(other)
            && self.namespace_id() == other.namespace_id()
            && self.subspace_id() == other.subspace_id()
            && self.path().is_prefix_of(other.path())
    }

    /// Returns whether this entry would be [prefix pruned](https://willowprotocol.org/specs/data-model/index.html#prefix_pruning) by another entry.
    ///
    /// Prefix pruning powers deletion in Willow: whenever a data store would contain two entries, one of which pruens the other, the other is removed from the data store (or never inserted in the first place). Informally speaking, newer entries remove older entries, but only if they are in the same namespace and subspace, and only if the path of the newer entry is a prefix of the path of the older entry.
    ///
    /// More precisely: an entry `e1` prunes an entry `e2` if and only if
    ///
    /// - `e1.namespace_id() == e2.namespace_id()`,
    /// - `e1.subspace_id() == e2.subspace_id()`,
    /// - `e1.path().is_prefix_of(e2.path())`, and
    /// - `e1.is_newer_than(&e2)`.
    ///
    /// This method is the reciprocal of [`EntrylikeExt::prunes`].
    ///
    /// # Examples
    ///
    ///
    /// ```
    /// use willow_data_model_generic::prelude::*;
    /// use willow_data_model_generic::prelude::*;
    ///
    /// let entry = Entry::builder()
    ///     .namespace_id("family")
    ///     .subspace_id("alfie")
    ///     .path(Path::<4, 4, 4>::from_slices(&["a", "b"])?)
    ///     .timestamp(12345)
    ///     .payload_digest("b")
    ///     .payload_length(17)
    ///     .build().unwrap();
    ///
    /// let newer = Entry::prefilled_builder(&entry).timestamp(99999).build().unwrap();
    /// assert!(entry.is_pruned_by(&newer));
    /// assert!(!newer.is_pruned_by(&entry));
    ///
    /// let newer_and_prefix = Entry::prefilled_builder(&newer)
    ///     .path(Path::<4, 4, 4>::from_slices(&["a"])?).build().unwrap();
    /// assert!(entry.is_pruned_by(&newer_and_prefix));
    /// assert!(!newer_and_prefix.is_pruned_by(&entry));
    ///
    /// let newer_and_extension = Entry::prefilled_builder(&newer)
    ///     .path(Path::<4, 4, 4>::from_slices(&["a", "b", "c"])?).build().unwrap();
    /// assert!(!entry.is_pruned_by(&newer_and_extension));
    /// assert!(!newer_and_extension.is_pruned_by(&entry));
    ///
    /// let newer_but_unrelated_namespace = Entry::prefilled_builder(&newer)
    ///     .namespace_id("bookclub").build().unwrap();
    /// assert!(!entry.is_pruned_by(&newer_but_unrelated_namespace));
    /// assert!(!newer_but_unrelated_namespace.is_pruned_by(&entry));
    ///
    /// let newer_but_unrelated_subspace = Entry::prefilled_builder(&newer)
    ///     .subspace_id("betty").build().unwrap();
    /// assert!(!entry.is_pruned_by(&newer_but_unrelated_subspace));
    /// assert!(!newer_but_unrelated_subspace.is_pruned_by(&entry));
    /// # Ok::<(), PathError>(())
    /// ```
    ///
    fn is_pruned_by<OtherEntry>(&self, other: &OtherEntry) -> bool
    where
        OtherEntry: Entrylike<MCL, MCC, MPL, N, S, PD>,
        N: PartialEq,
        S: PartialEq,
        PD: Ord,
        Self: Sized,
    {
        other.prunes(self)
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, N, S, PD, E>
    EntrylikeExt<MCL, MCC, MPL, N, S, PD> for E
where
    E: Entrylike<MCL, MCC, MPL, N, S, PD>,
{
}
