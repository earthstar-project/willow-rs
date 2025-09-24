use core::cmp;
use core::cmp::Ordering;

#[cfg(feature = "dev")]
use arbitrary::{Arbitrary, Error as ArbitraryError};

use crate::path::Path;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
/// Determines whether a [`Range`] is _closed_ or _open_.
pub enum RangeEnd<T> {
    /// A [closed range](https://willowprotocol.org/specs/grouping-entries/index.html#closed_range) consists of a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value) and an [end_value](https://willowprotocol.org/specs/grouping-entries/index.html#end_value).
    Closed(T),
    /// An [open range](https://willowprotocol.org/specs/grouping-entries/index.html#open_range) consists only of a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    Open,
}

impl<T: PartialOrd> RangeEnd<T> {
    /// Return if the [`RangeEnd`] is greater than the given value.
    pub fn gt_val(&self, val: &T) -> bool {
        match self {
            RangeEnd::Open => true,
            RangeEnd::Closed(end_val) => end_val > val,
        }
    }
}

impl From<&RangeEnd<u64>> for u64 {
    fn from(value: &RangeEnd<u64>) -> Self {
        match value {
            RangeEnd::Closed(val) => *val,
            RangeEnd::Open => u64::MAX,
        }
    }
}

impl<T: Ord> Ord for RangeEnd<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        match self {
            RangeEnd::Closed(end_value) => match other {
                RangeEnd::Closed(other_end_value) => end_value.cmp(other_end_value),
                RangeEnd::Open => Ordering::Less,
            },
            RangeEnd::Open => match other {
                RangeEnd::Closed(_) => Ordering::Greater,
                RangeEnd::Open => Ordering::Equal,
            },
        }
    }
}

impl<T: PartialOrd> PartialOrd for RangeEnd<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self {
            RangeEnd::Closed(end_value) => match other {
                RangeEnd::Closed(other_end_value) => end_value.partial_cmp(other_end_value),
                RangeEnd::Open => Some(Ordering::Less),
            },
            RangeEnd::Open => match other {
                RangeEnd::Closed(_) => Some(Ordering::Greater),
                RangeEnd::Open => Some(Ordering::Equal),
            },
        }
    }
}

impl<T> PartialEq<T> for RangeEnd<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        match self {
            RangeEnd::Closed(val) => val.eq(other),
            RangeEnd::Open => false,
        }
    }
}

impl<T> PartialOrd<T> for RangeEnd<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &T) -> Option<Ordering> {
        match self {
            RangeEnd::Closed(val) => val.partial_cmp(other),
            RangeEnd::Open => Some(Ordering::Greater),
        }
    }
}

impl PartialEq<RangeEnd<u64>> for u64 {
    fn eq(&self, other: &RangeEnd<u64>) -> bool {
        match other {
            RangeEnd::Closed(other_val) => self.eq(other_val),
            RangeEnd::Open => false,
        }
    }
}

impl PartialOrd<RangeEnd<u64>> for u64 {
    fn partial_cmp(&self, other: &RangeEnd<u64>) -> Option<Ordering> {
        match other {
            RangeEnd::Closed(other_val) => self.partial_cmp(other_val),
            RangeEnd::Open => Some(Ordering::Less),
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialEq<RangeEnd<Path<MCL, MCC, MPL>>>
    for Path<MCL, MCC, MPL>
{
    fn eq(&self, other: &RangeEnd<Path<MCL, MCC, MPL>>) -> bool {
        match other {
            RangeEnd::Closed(other_path) => self.eq(other_path),
            RangeEnd::Open => false,
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialOrd<RangeEnd<Path<MCL, MCC, MPL>>>
    for Path<MCL, MCC, MPL>
{
    fn partial_cmp(&self, other: &RangeEnd<Path<MCL, MCC, MPL>>) -> Option<Ordering> {
        match other {
            RangeEnd::Closed(other_path) => self.partial_cmp(other_path),
            RangeEnd::Open => Some(Ordering::Less),
        }
    }
}

/// One-dimensional grouping over a type of value.
///
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#range)
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct Range<T> {
    /// A range [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) all values greater than or equal to its [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value) **and** less than its [end_value](https://willowprotocol.org/specs/grouping-entries/index.html#end_value)
    pub start: T,
    /// A range [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) all values strictly less than its end value (if it is not open) **and** greater than or equal to its [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    pub end: RangeEnd<T>,
}

impl<T> Range<T> {
    /// Returns a new [`Range`].
    pub fn new(start: T, end: RangeEnd<T>) -> Self {
        Self { start, end }
    }

    /// Returns a new [open range](https://willowprotocol.org/specs/grouping-entries/index.html#open_range) from a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    pub fn new_open(start: T) -> Self {
        Self {
            start,
            end: RangeEnd::Open,
        }
    }

    /// Returns whether this range is open.
    pub fn is_open(&self) -> bool {
        match self.end {
            RangeEnd::Open => true,
            _ => false,
        }
    }

    /// Returns the end value if the range is closed, or `None` if the range is open.
    pub fn get_end(&self) -> Option<&T> {
        match self.end {
            RangeEnd::Open => None,
            RangeEnd::Closed(ref t) => Some(t),
        }
    }
}

impl<T> Range<T>
where
    T: PartialOrd,
{
    /// Returns a new [closed range](https://willowprotocol.org/specs/grouping-entries/index.html#closed_range) from a [start](https://willowprotocol.org/specs/grouping-entries/index.html#start_value) and [end_value](https://willowprotocol.org/specs/grouping-entries/index.html#end_value), or [`None`] if the resulting range would never [include](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) any values.
    pub fn new_closed(start: T, end: T) -> Option<Self> {
        if start < end {
            return Some(Self {
                start,
                end: RangeEnd::Closed(end),
            });
        }

        None
    }

    /// Returns whether a given value is [included](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) by the [`Range`] or not.
    pub fn includes(&self, value: &T) -> bool {
        &self.start <= value && self.end.gt_val(value)
    }

    /// Returns `true` if `other` range is fully [included](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) in this [`Range`].
    pub fn includes_range(&self, other: &Range<T>) -> bool {
        self.start <= other.start && self.end >= other.end
    }
}
impl<T> Range<T>
where
    T: Ord + Clone,
{
    /// Creates the [intersection](https://willowprotocol.org/specs/grouping-entries/index.html#range_intersection) between this [`Range`] and another `Range`, if any.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = cmp::max(&self.start, &other.start);
        let end = match (&self.end, &other.end) {
            (RangeEnd::Open, RangeEnd::Closed(b)) => RangeEnd::Closed(b),
            (RangeEnd::Closed(a), RangeEnd::Closed(b)) => RangeEnd::Closed(a.min(b)),
            (RangeEnd::Closed(a), RangeEnd::Open) => RangeEnd::Closed(a),
            (RangeEnd::Open, RangeEnd::Open) => RangeEnd::Open,
        };
        match end {
            RangeEnd::Open => Some(Self::new_open(start.clone())),
            RangeEnd::Closed(t) => Self::new_closed(start.clone(), t.clone()),
        }
    }
}

impl<T: Default> Range<T> {
    /// Creates a new range which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) everything. This requires `Default::default()` to return the minimal value of type `T` in order to be correct.
    pub fn full() -> Self {
        Self::new_open(T::default())
    }
}

impl<T: Default> Default for Range<T> {
    fn default() -> Self {
        Self::full()
    }
}

impl<T: Ord> Ord for Range<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.start.cmp(&other.start) {
            Ordering::Less => Ordering::Less,
            Ordering::Equal => Ordering::Equal,
            Ordering::Greater => self.end.cmp(&other.end),
        }
    }
}

impl<T: PartialOrd> PartialOrd for Range<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.start.partial_cmp(&other.start) {
            None => None,
            Some(Ordering::Less) => Some(Ordering::Less),
            Some(Ordering::Equal) => Some(Ordering::Equal),
            Some(Ordering::Greater) => self.end.partial_cmp(&other.end),
        }
    }
}

#[cfg(feature = "dev")]
impl<'a, T> Arbitrary<'a> for Range<T>
where
    T: Arbitrary<'a> + PartialOrd,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let start: T = Arbitrary::arbitrary(u)?;
        let end: RangeEnd<T> = Arbitrary::arbitrary(u)?;

        if !end.gt_val(&start) {
            return Err(ArbitraryError::IncorrectFormat);
        }

        Ok(Self { start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Range end

    #[test]
    fn range_end_ord() {
        assert!(RangeEnd::Closed(0) == RangeEnd::Closed(0));
        assert!(RangeEnd::Closed(1) < RangeEnd::Closed(2));
        assert!(RangeEnd::Closed(100) < RangeEnd::Open);
        assert!(RangeEnd::<usize>::Open == RangeEnd::Open);
    }

    // Range

    #[test]
    fn range_new_closed() {
        assert!(Range::new_closed(0, 0).is_none());
        assert!(Range::new_closed(2, 1).is_none());
        assert!(Range::new_closed(5, 10).is_some());
    }

    #[test]
    fn range_includes() {
        let open_range = Range::new_open(20);

        assert!(open_range.includes(&20));
        assert!(open_range.includes(&30));
        assert!(!open_range.includes(&10));

        let closed_range = Range::new_closed(5, 10).unwrap();

        assert!(closed_range.includes(&5));
        assert!(closed_range.includes(&8));
        assert!(!closed_range.includes(&1));
    }

    #[test]
    fn range_includes_range() {
        let open_range = Range::new_open(0);
        let open_range_2 = Range::new_open(2);

        assert!(open_range.includes_range(&open_range_2));
        assert!(!open_range_2.includes_range(&open_range));

        let closed_range = Range::new_closed(0, 10).unwrap();
        let closed_range_2 = Range::new_closed(5, 10).unwrap();
        let closed_range_3 = Range::new_closed(5, 15).unwrap();

        assert!(closed_range.includes_range(&closed_range_2));
        assert!(!closed_range_2.includes_range(&closed_range));
        assert!(!closed_range.includes_range(&closed_range_3));

        assert!(open_range.includes_range(&closed_range));
        assert!(!closed_range.includes_range(&open_range_2));
    }

    #[test]
    fn range_intersection() {
        let closed_1 = Range::new_closed(0, 10).unwrap();
        let closed_2 = Range::new_closed(5, 15).unwrap();

        let intersection_1 = closed_1.intersection(&closed_2);

        assert!(intersection_1.is_some());
        assert_eq!(intersection_1.unwrap(), Range::new_closed(5, 10).unwrap());

        let open_1 = Range::new_open(5);

        let intersection_2 = open_1.intersection(&closed_1);

        assert!(intersection_2.is_some());
        assert_eq!(intersection_2.unwrap(), Range::new_closed(5, 10).unwrap());

        let closed_3 = Range::new_closed(20, 25).unwrap();

        let intersection_3 = closed_3.intersection(&closed_1);

        assert!(intersection_3.is_none());

        let open_2 = Range::new_open(50);

        let intersection_4 = closed_3.intersection(&open_2);

        assert!(intersection_4.is_none())
    }
}
