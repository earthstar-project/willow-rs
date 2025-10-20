/// A trait for total orders where each but at most one element has a unique successor, i.e., a least greater value.
///
/// This trait is in this crate because converting [Areas](https://willowprotocol.org/specs/grouping-entries/index.html#areas) into [3d ranges](https://willowprotocol.org/specs/grouping-entries/index.html#D3Range) relies on successors existing.
pub trait Successor: Ord + Sized {
    /// Returns the successor, i.e., the unique least value which is strictly greater than `self`. If `self` is the greatest possible value, this returns `None` instead.
    fn successor(&self) -> Option<Self>;
}

impl Successor for u64 {
    fn successor(&self) -> Option<Self> {
        self.checked_add(1)
    }
}

/// A trait for total orders with a unique least element.
///
/// This trait is in this crate because some Willow specifications rely on the existence of unique least NamespaceIds and SubspaceIds.
pub trait Minimum: Ord + Sized {
    /// Returns the minimum, i.e., the unique least element.
    fn minimum() -> Self;
}

impl Minimum for u64 {
    fn minimum() -> Self {
        0
    }
}

/// A trait for total orders with a unique greatest element.
///
/// This trait is in this crate because working with [ranges](https://willowprotocol.org/specs/grouping-entries/index.html#ranges) requires knowing greatest possible values.
pub trait Maximum: Ord + Sized {
    /// Returns the maximum, i.e., the unique greatest element.
    fn maximum() -> Self;
}

impl Maximum for u64 {
    fn maximum() -> Self {
        u64::MAX
    }
}
