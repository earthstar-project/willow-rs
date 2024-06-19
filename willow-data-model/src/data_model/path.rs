use std::rc::Rc;

#[derive(Debug)]
/// An error indicating a [`PathComponent`'s bytestring is too long.
pub struct ComponentTooLongError;

/// A bytestring representing an individual component of a [`Path`]. Provides access to the raw bytes via the [`AsRef<u8>`] trait.
///
/// ## Implementation notes
///
/// - This trait provides an immutable interface, so cloning of a [`PathComponent`] implementation is expected to be cheap.
/// - Implementations of the [`Ord`] trait must correspond to lexicographically comparing the AsRef bytes.
pub trait PathComponent: Eq + AsRef<[u8]> + Clone + PartialOrd + Ord {
    /// The maximum bytelength of a path component, corresponding to Willow's [`max_component_length`](https://willowprotocol.org/specs/data-model/index.html#max_component_length) parameter.
    const MAX_COMPONENT_LENGTH: usize;

    /// Construct a new [`PathComponent`] from the provided slice, or return a [`ComponentTooLongError`] if the resulting component would be longer than [`PathComponent::MAX_COMPONENT_LENGTH`].
    fn new(components: &[u8]) -> Result<Self, ComponentTooLongError>;

    /// The length of the component's `as_ref` bytes.
    fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Whether the component is an empty bytestring or not.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug)]
/// An error arising from trying to construct a invalid [`Path`] from valid [`PathComponent`].
pub enum InvalidPathError {
    /// The path's total length in bytes is too large.
    PathTooLong,
    /// The path has too many components.
    TooManyComponents,
}

/// A sequence of [`PathComponent`], used to name and query entries in [Willow's data model](https://willowprotocol.org/specs/data-model/index.html#data_model).
///
pub trait Path: PartialEq + Eq + PartialOrd + Ord + Clone {
    type Component: PathComponent;

    /// The maximum number of [`PathComponent`] a [`Path`] may have, corresponding to Willow's [`max_component_count`](https://willowprotocol.org/specs/data-model/index.html#max_component_count) parameter
    const MAX_COMPONENT_COUNT: usize;
    /// The maximum total number of bytes a [`Path`] may have, corresponding to Willow's [`max_path_length`](https://willowprotocol.org/specs/data-model/index.html#max_path_length) parameter
    const MAX_PATH_LENGTH: usize;

    /// Contruct a new [`Path`] from a slice of [`PathComponent`], or return a [`InvalidPathError`] if the resulting path would be too long or have too many components.
    fn new(components: &[Self::Component]) -> Result<Self, InvalidPathError>;

    /// Construct a new [`Path`] with no components.
    fn empty() -> Self;

    /// Return a new [`Path`] with the components of this path suffixed with the given [`PathComponent`](PathComponent), or return [`InvalidPathError`] if the resulting path would be invalid in some way.
    fn append(&self, component: Self::Component) -> Result<Self, InvalidPathError>;

    /// Return an iterator of all this path's components.
    fn components(&self) -> impl Iterator<Item = &Self::Component>;

    /// Create a new [`Path`] by taking the first `length` components of this path.
    fn create_prefix(&self, length: usize) -> Self;

    /// Return all possible prefixes of a path, including the empty path and the path itself.
    fn all_prefixes(&self) -> impl Iterator<Item = Self> {
        let self_len = self.components().count();

        (0..=self_len).map(|i| self.create_prefix(i))
    }

    /// Test whether this path is a prefix of the given path.
    /// Paths are always a prefix of themselves.
    fn is_prefix_of(&self, other: &Self) -> bool {
        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return false;
            }
        }

        true
    }

    /// Test whether this path is prefixed by the given path.
    /// Paths are always a prefix of themselves.
    fn is_prefixed_by(&self, other: &Self) -> bool {
        other.is_prefix_of(self)
    }

    /// Return the longest common prefix of this path and the given path.
    fn longest_common_prefix(&self, other: &Self) -> Self {
        let mut lcp_len = 0;

        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                break;
            }

            lcp_len += 1
        }

        self.create_prefix(lcp_len)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathComponentLocal<const MCL: usize>(Box<[u8]>);

/// An implementation of [`PathComponent`] for [`PathComponentLocal`].
///
/// ## Type parameters
///
/// - `MCL`: A [`usize`] used as [`PathComponent::MAX_COMPONENT_LENGTH`].
impl<const MCL: usize> PathComponent for PathComponentLocal<MCL> {
    const MAX_COMPONENT_LENGTH: usize = MCL;

    /// Create a new component by cloning and appending all bytes from the slice into a [`Vec<u8>`], or return a [`ComponentTooLongError`] if the bytelength of the slice exceeds [`PathComponent::MAX_COMPONENT_LENGTH`].
    fn new(bytes: &[u8]) -> Result<Self, ComponentTooLongError> {
        if bytes.len() > Self::MAX_COMPONENT_LENGTH {
            return Err(ComponentTooLongError);
        }

        Ok(Self(bytes.into()))
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<const MCL: usize> AsRef<[u8]> for PathComponentLocal<MCL> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
/// A cheaply cloneable [`Path`] using a `Rc<[PathComponentLocal]>`.
/// While cloning is cheap, operations which return modified forms of the path (e.g. [`Path::append`]) are not, as they have to clone the contents of the undelying [`Rc`].
pub struct PathLocal<const MCL: usize, const MCC: usize, const MPL: usize>(
    Rc<[PathComponentLocal<MCL>]>,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize> Path for PathLocal<MCL, MCC, MPL> {
    type Component = PathComponentLocal<MCL>;

    const MAX_COMPONENT_COUNT: usize = MCC;
    const MAX_PATH_LENGTH: usize = MPL;

    fn new(components: &[Self::Component]) -> Result<Self, InvalidPathError> {
        if components.len() > Self::MAX_COMPONENT_COUNT {
            return Err(InvalidPathError::TooManyComponents);
        };

        let mut path_vec = Vec::new();
        let mut total_length = 0;

        for component in components {
            total_length += component.len();

            if total_length > Self::MAX_PATH_LENGTH {
                return Err(InvalidPathError::PathTooLong);
            } else {
                path_vec.push(component.clone());
            }
        }

        Ok(PathLocal(path_vec.into()))
    }

    fn empty() -> Self {
        PathLocal(Vec::new().into())
    }

    fn create_prefix(&self, length: usize) -> Self {
        if length == 0 {
            return Self::empty();
        }

        let until = std::cmp::min(length, self.0.len());
        let slice = &self.0[0..until];

        Path::new(slice).unwrap()
    }

    fn append(&self, component: Self::Component) -> Result<Self, InvalidPathError> {
        let total_component_count = self.0.len();

        if total_component_count + 1 > MCC {
            return Err(InvalidPathError::TooManyComponents);
        }

        let total_path_length = self.0.iter().fold(0, |acc, item| acc + item.0.len());

        if total_path_length + component.as_ref().len() > MPL {
            return Err(InvalidPathError::PathTooLong);
        }

        let mut new_path_vec = Vec::new();

        for component in self.components() {
            new_path_vec.push(component.clone())
        }

        new_path_vec.push(component);

        Ok(PathLocal(new_path_vec.into()))
    }

    fn components(&self) -> impl Iterator<Item = &Self::Component> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MCL: usize = 8;
    const MCC: usize = 4;
    const MPL: usize = 16;

    #[test]
    fn empty() {
        let empty_path = PathLocal::<MCL, MCC, MPL>::empty();

        assert_eq!(empty_path.components().count(), 0);
    }

    #[test]
    fn new() {
        let component_too_long =
            PathComponentLocal::<MCL>::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'z']);

        assert!(matches!(component_too_long, Err(ComponentTooLongError)));

        let too_many_components = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'z']).unwrap(),
        ]);

        assert!(matches!(
            too_many_components,
            Err(InvalidPathError::TooManyComponents)
        ));

        let path_too_long = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']).unwrap(),
            PathComponentLocal::new(&[b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']).unwrap(),
            PathComponentLocal::new(&[b'z']).unwrap(),
        ]);

        assert!(matches!(path_too_long, Err(InvalidPathError::PathTooLong)));
    }

    #[test]
    fn append() {
        let path = PathLocal::<MCL, MCC, MPL>::empty();

        let r1 = path.append(PathComponentLocal::new(&[b'a']).unwrap());
        assert!(r1.is_ok());
        let p1 = r1.unwrap();
        assert_eq!(p1.components().count(), 1);

        let r2 = p1.append(PathComponentLocal::new(&[b'b']).unwrap());
        assert!(r2.is_ok());
        let p2 = r2.unwrap();
        assert_eq!(p2.components().count(), 2);

        let r3 = p2.append(PathComponentLocal::new(&[b'c']).unwrap());
        assert!(r3.is_ok());
        let p3 = r3.unwrap();
        assert_eq!(p3.components().count(), 3);

        let r4 = p3.append(PathComponentLocal::new(&[b'd']).unwrap());
        assert!(r4.is_ok());
        let p4 = r4.unwrap();
        assert_eq!(p4.components().count(), 4);

        let r5 = p4.append(PathComponentLocal::new(&[b'z']).unwrap());
        assert!(r5.is_err());

        let collected = p4
            .components()
            .map(|comp| comp.as_ref())
            .collect::<Vec<&[u8]>>();

        assert_eq!(collected, vec![[b'a'], [b'b'], [b'c'], [b'd'],])
    }

    #[test]
    fn prefix() {
        let path = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let prefix0 = path.create_prefix(0);

        assert_eq!(prefix0, PathLocal::empty());

        let prefix1 = path.create_prefix(1);

        assert_eq!(
            prefix1,
            PathLocal::<MCL, MCC, MPL>::new(&[PathComponentLocal::new(&[b'a']).unwrap()]).unwrap()
        );

        let prefix2 = path.create_prefix(2);

        assert_eq!(
            prefix2,
            PathLocal::<MCL, MCC, MPL>::new(&[
                PathComponentLocal::new(&[b'a']).unwrap(),
                PathComponentLocal::new(&[b'b']).unwrap()
            ])
            .unwrap()
        );

        let prefix3 = path.create_prefix(3);

        assert_eq!(
            prefix3,
            PathLocal::<MCL, MCC, MPL>::new(&[
                PathComponentLocal::new(&[b'a']).unwrap(),
                PathComponentLocal::new(&[b'b']).unwrap(),
                PathComponentLocal::new(&[b'c']).unwrap()
            ])
            .unwrap()
        );

        let prefix4 = path.create_prefix(4);

        assert_eq!(
            prefix4,
            PathLocal::<MCL, MCC, MPL>::new(&[
                PathComponentLocal::new(&[b'a']).unwrap(),
                PathComponentLocal::new(&[b'b']).unwrap(),
                PathComponentLocal::new(&[b'c']).unwrap()
            ])
            .unwrap()
        )
    }

    #[test]
    fn prefixes() {
        let path = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let prefixes: Vec<PathLocal<MCL, MCC, MPL>> = path.all_prefixes().collect();

        assert_eq!(
            prefixes,
            vec![
                PathLocal::<MCL, MCC, MPL>::new(&[]).unwrap(),
                PathLocal::<MCL, MCC, MPL>::new(&[PathComponentLocal::new(&[b'a']).unwrap()])
                    .unwrap(),
                PathLocal::<MCL, MCC, MPL>::new(&[
                    PathComponentLocal::new(&[b'a']).unwrap(),
                    PathComponentLocal::new(&[b'b']).unwrap()
                ])
                .unwrap(),
                PathLocal::<MCL, MCC, MPL>::new(&[
                    PathComponentLocal::new(&[b'a']).unwrap(),
                    PathComponentLocal::new(&[b'b']).unwrap(),
                    PathComponentLocal::new(&[b'c']).unwrap()
                ])
                .unwrap(),
            ]
        )
    }

    #[test]
    fn is_prefix_of() {
        let path_a = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
        ])
        .unwrap();

        let path_b = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let path_c = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'x']).unwrap(),
            PathComponentLocal::new(&[b'y']).unwrap(),
            PathComponentLocal::new(&[b'z']).unwrap(),
        ])
        .unwrap();

        assert!(path_a.is_prefix_of(&path_b));
        assert!(!path_a.is_prefix_of(&path_c));
    }

    #[test]
    fn is_prefixed_by() {
        let path_a = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
        ])
        .unwrap();

        let path_b = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let path_c = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'x']).unwrap(),
            PathComponentLocal::new(&[b'y']).unwrap(),
            PathComponentLocal::new(&[b'z']).unwrap(),
        ])
        .unwrap();

        assert!(path_b.is_prefixed_by(&path_a));
        assert!(!path_c.is_prefixed_by(&path_a));
    }

    #[test]
    fn longest_common_prefix() {
        let path_a = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'x']).unwrap(),
        ])
        .unwrap();

        let path_b = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'b']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let path_c = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'x']).unwrap(),
            PathComponentLocal::new(&[b'y']).unwrap(),
            PathComponentLocal::new(&[b'z']).unwrap(),
        ])
        .unwrap();

        let lcp_a_b = path_a.longest_common_prefix(&path_b);

        assert_eq!(
            lcp_a_b,
            PathLocal::<MCL, MCC, MPL>::new(&[PathComponentLocal::new(&[b'a']).unwrap()]).unwrap()
        );

        let lcp_b_a = path_b.longest_common_prefix(&path_a);

        assert_eq!(lcp_b_a, lcp_a_b);

        let lcp_a_x = path_a.longest_common_prefix(&path_c);

        assert_eq!(lcp_a_x, PathLocal::empty());

        let path_d = PathLocal::<MCL, MCC, MPL>::new(&[
            PathComponentLocal::new(&[b'a']).unwrap(),
            PathComponentLocal::new(&[b'x']).unwrap(),
            PathComponentLocal::new(&[b'c']).unwrap(),
        ])
        .unwrap();

        let lcp_b_d = path_b.longest_common_prefix(&path_d);

        assert_eq!(
            lcp_b_d,
            PathLocal::<MCL, MCC, MPL>::new(&[PathComponentLocal::new(&[b'a']).unwrap()]).unwrap()
        )
    }
}
