use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
/// Determines whether a [`Range`] is _closed_ or _open_.
pub enum RangeEnd<T>
where
    T: PartialEq + Eq + PartialOrd + Ord,
{
    /// A [closed range](https://willowprotocol.org/specs/grouping-entries/index.html#closed_range) consists of a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value) and an [end_value](https://willowprotocol.org/specs/grouping-entries/index.html#end_value).
    Closed(T),
    /// An [open range](https://willowprotocol.org/specs/grouping-entries/index.html#open_range) consists only of a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    Open,
}

impl<T> RangeEnd<T>
where
    T: PartialEq + Eq + PartialOrd + Ord,
{
    /// Return if the [`RangeEnd`] is greater than the given value.
    pub fn gt_val(&self, val: &T) -> bool {
        match self {
            RangeEnd::Open => true,
            RangeEnd::Closed(end_val) => end_val > val,
        }
    }
}

impl<T> Ord for RangeEnd<T>
where
    T: PartialEq + Eq + PartialOrd + Ord,
{
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

impl<T: Ord + PartialOrd> PartialOrd for RangeEnd<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One-dimensional grouping over a type of value.
///
/// [Definition](https://willowprotocol.org/specs/grouping-entries/index.html#range)
#[derive(Debug, PartialEq, Eq)]
pub struct Range<T>
where
    T: PartialEq + Eq + PartialOrd + Ord,
{
    /// A range [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) all values greater than or equal to its [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    pub start: T,
    /// A range [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) all values strictly less than its end value (if it is not open).
    pub end: RangeEnd<T>,
}

/// An error indicating a range is [empty](https://willowprotocol.org/specs/grouping-entries/index.html#range_empty) and will never [include](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) any values.
#[derive(Debug)]
pub struct EmptyRangeError;

impl<T> Range<T>
where
    T: PartialEq + Eq + PartialOrd + Ord + Clone,
{
    /// Construct a new [open range](https://willowprotocol.org/specs/grouping-entries/index.html#open_range) from a [start value](https://willowprotocol.org/specs/grouping-entries/index.html#start_value).
    pub fn new_open(start: T) -> Self {
        Self {
            start,
            end: RangeEnd::Open,
        }
    }

    /// Construct a new [closed range](https://willowprotocol.org/specs/grouping-entries/index.html#closed_range) from a [start](https://willowprotocol.org/specs/grouping-entries/index.html#start_value) and [end_value](https://willowprotocol.org/specs/grouping-entries/index.html#end_value), or a [`UselessRangeError`] if the resulting range would never [include](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) any values.
    pub fn new_closed(start: T, end: T) -> Result<Self, EmptyRangeError> {
        if start < end {
            return Ok(Self {
                start,
                end: RangeEnd::Closed(end),
            });
        }

        Err(EmptyRangeError)
    }

    /// Return whether a given value is [included](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) by the [`Range`] or not.
    pub fn includes(&self, value: T) -> bool {
        self.start <= value && self.end.gt_val(&value)
    }

    /// Returns `true` if `other` range is fully [included](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) in this [`Range`].
    pub fn includes_range(&self, other: &Range<T>) -> bool {
        self.start <= other.start && self.end >= other.end
    }

    /// Create the [intersection](https://willowprotocol.org/specs/grouping-entries/index.html#range_intersection) between this [`Range`] and another `Range`.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = (&self.start).max(&other.start);
        let end = match (&self.end, &other.end) {
            (RangeEnd::Open, RangeEnd::Closed(b)) => RangeEnd::Closed(b),
            (RangeEnd::Closed(a), RangeEnd::Closed(b)) => RangeEnd::Closed(a.min(b)),
            (RangeEnd::Closed(a), RangeEnd::Open) => RangeEnd::Closed(a),
            (RangeEnd::Open, RangeEnd::Open) => RangeEnd::Open,
        };
        match end {
            RangeEnd::Open => Some(Self::new_open(start.clone())),
            RangeEnd::Closed(t) if t >= start => {
                Some(Self::new_closed(start.clone(), t.clone()).unwrap())
            }
            RangeEnd::Closed(_) => None,
        }
    }
}

impl<T: Default> Range<T>
where
    T: PartialEq + Eq + PartialOrd + Ord + Clone,
{
    /// Create a new range which [includes](https://willowprotocol.org/specs/grouping-entries/index.html#range_include) everything.
    pub fn full() -> Self {
        Self::new_open(T::default())
    }
}

impl<T: Default> Default for Range<T>
where
    T: PartialEq + Eq + PartialOrd + Ord + Clone,
{
    fn default() -> Self {
        Self::full()
    }
}

impl<T> Ord for Range<T>
where
    T: PartialEq + Eq + PartialOrd + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        match self.start.cmp(&other.start) {
            Ordering::Less => Ordering::Less,
            Ordering::Equal => Ordering::Equal,
            Ordering::Greater => self.end.cmp(&other.end),
        }
    }
}

impl<T: Ord + PartialOrd> PartialOrd for Range<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
