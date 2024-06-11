#[derive(Debug)]
pub enum InvalidPathError {
    ComponentTooLong(usize),
    PathTooLong,
    TooManyComponents,
}

#[derive(Debug)]
pub enum AppendPathError {
    PathTooLong,
    TooManyComponents,
}

pub trait Path: PartialEq + Eq + PartialOrd + Ord + Clone {
    type Component: Eq + AsRef<[u8]> + Clone + PartialOrd + Ord;

    const MAX_COMPONENT_LENGTH: usize;
    const MAX_COMPONENT_COUNT: usize;
    const MAX_PATH_LENGTH: usize;

    fn empty() -> Self;

    fn validate(path: &Self) -> Result<&Self, InvalidPathError> {
        if path.components().count() > Self::MAX_COMPONENT_COUNT {
            return Err(InvalidPathError::TooManyComponents);
        }

        let mut total_length = 0;

        for (i, component) in path.components().enumerate() {
            let length = component.as_ref().len();
            if length > Self::MAX_COMPONENT_LENGTH {
                return Err(InvalidPathError::ComponentTooLong(i));
            }
            total_length += length;
        }

        if total_length > Self::MAX_PATH_LENGTH {
            return Err(InvalidPathError::PathTooLong);
        }

        Ok(path)
    }

    fn append(&mut self, component: Self::Component) -> Result<(), AppendPathError>;

    fn components(&self) -> impl Iterator<Item = &Self::Component>;

    fn prefix(&self, length: usize) -> Self {
        if length == 0 {
            return Self::empty();
        }

        if length > self.components().count() {
            return self.clone();
        }

        let mut new_path: Self = Self::empty();

        for (i, component) in self.components().enumerate() {
            new_path.append(component.clone()).unwrap();

            if i + 1 >= length {
                break;
            }
        }

        new_path
    }

    fn prefixes(&self) -> Vec<Self> {
        let self_len = self.components().count();

        (0..=self_len).map(|i| self.prefix(i)).collect()
    }

    fn is_prefix_of(&self, other: &Self) -> bool {
        let lcp = self.longest_common_prefix(other);

        lcp.components().count() == self.components().count()
    }

    fn is_prefixed_by(&self, other: &Self) -> bool {
        let lcp = self.longest_common_prefix(other);

        lcp.components().count() == other.components().count()
    }

    fn longest_common_prefix(&self, other: &Self) -> Self {
        let mut new_path = Self::empty();

        self.components()
            .zip(other.components())
            .for_each(|(a_comp, b_comp)| {
                if a_comp == b_comp {
                    new_path.append(a_comp.clone()).unwrap();
                }
            });

        new_path
    }
}

// Single threaded default.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathComponentLocal<const MCL: usize>(Vec<u8>);

#[derive(Debug)]
pub struct ComponentTooLongError;

impl<const MCL: usize> PathComponentLocal<MCL> {
    pub fn new(bytes: &[u8]) -> Result<Self, ComponentTooLongError> {
        if bytes.len() > MCL {
            return Err(ComponentTooLongError);
        }

        let mut vec = Vec::new();
        vec.extend_from_slice(bytes);

        Ok(Self(vec))
    }
}

impl<const MCL: usize> AsRef<[u8]> for PathComponentLocal<MCL> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct PathLocal<const MCL: usize, const MCC: usize, const MPL: usize>(
    Vec<PathComponentLocal<MCL>>,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize> Path for PathLocal<MCL, MCC, MPL> {
    type Component = PathComponentLocal<MCL>;

    const MAX_COMPONENT_LENGTH: usize = MCL;
    const MAX_COMPONENT_COUNT: usize = MCC;
    const MAX_PATH_LENGTH: usize = MPL;

    fn empty() -> Self {
        PathLocal(Vec::new())
    }

    fn append(&mut self, component: Self::Component) -> Result<(), AppendPathError> {
        self.0.push(component);

        match Path::validate(self) {
            Ok(_) => Ok(()),
            Err(err) => {
                self.0.pop();
                match err {
                    InvalidPathError::ComponentTooLong(_) => panic!("Component length was too long for a component that was apparently not too long?!"),
                    InvalidPathError::PathTooLong => Err(AppendPathError::PathTooLong),
                    InvalidPathError::TooManyComponents => Err(AppendPathError::TooManyComponents),
                }
            }
        }
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
    fn validate() {
        let empty_path = PathLocal::<MCL, MCC, MPL>::empty();
        let empty_is_valid = PathLocal::<MCL, MCC, MPL>::validate(&empty_path);

        assert!(empty_is_valid.is_ok());

        let component_too_long = PathLocal::<MCL, MCC, MPL>(vec![PathComponentLocal(vec![
            b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'z',
        ])]);
        let component_too_long_valid = PathLocal::<MCL, MCC, MPL>::validate(&component_too_long);

        assert!(matches!(
            component_too_long_valid,
            Err(InvalidPathError::ComponentTooLong(0))
        ));

        let too_many_components = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'z']),
        ]);
        let too_many_components_valid = PathLocal::<MCL, MCC, MPL>::validate(&too_many_components);

        assert!(matches!(
            too_many_components_valid,
            Err(InvalidPathError::TooManyComponents)
        ));

        let path_too_long = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']),
            PathComponentLocal(vec![b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']),
            PathComponentLocal(vec![b'z']),
        ]);
        let path_too_long_valid = PathLocal::<MCL, MCC, MPL>::validate(&path_too_long);

        assert!(matches!(
            path_too_long_valid,
            Err(InvalidPathError::PathTooLong)
        ));
    }

    #[test]
    fn append() {
        let mut path = PathLocal::<MCL, MCC, MPL>::empty();

        let r1 = path.append(PathComponentLocal(vec![b'a']));
        assert!(r1.is_ok());
        assert_eq!(path.components().count(), 1);
        let r2 = path.append(PathComponentLocal(vec![b'b']));
        assert!(r2.is_ok());
        assert_eq!(path.components().count(), 2);
        let r3 = path.append(PathComponentLocal(vec![b'c']));
        assert!(r3.is_ok());
        assert_eq!(path.components().count(), 3);
        let r4 = path.append(PathComponentLocal(vec![b'd']));
        assert!(r4.is_ok());
        assert_eq!(path.components().count(), 4);
        let r5 = path.append(PathComponentLocal(vec![b'z']));
        assert!(r5.is_err());

        let collected = path
            .components()
            .map(|comp| comp.as_ref())
            .collect::<Vec<&[u8]>>();

        assert_eq!(collected, vec![[b'a'], [b'b'], [b'c'], [b'd'],])
    }

    #[test]
    fn prefix() {
        let path = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let prefix0 = path.prefix(0);

        assert_eq!(prefix0, PathLocal::empty());

        let prefix1 = path.prefix(1);

        assert_eq!(
            prefix1,
            PathLocal::<MCL, MCC, MPL>(vec![PathComponentLocal(vec![b'a'])])
        );

        let prefix2 = path.prefix(2);

        assert_eq!(
            prefix2,
            PathLocal::<MCL, MCC, MPL>(vec![
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b'])
            ])
        );

        let prefix3 = path.prefix(3);

        assert_eq!(
            prefix3,
            PathLocal::<MCL, MCC, MPL>(vec![
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b']),
                PathComponentLocal(vec![b'c'])
            ])
        );

        let prefix4 = path.prefix(4);

        assert_eq!(
            prefix4,
            PathLocal::<MCL, MCC, MPL>(vec![
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b']),
                PathComponentLocal(vec![b'c'])
            ])
        )
    }

    #[test]
    fn prefixes() {
        let path = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let prefixes = path.prefixes();

        assert_eq!(
            prefixes,
            vec![
                PathLocal::<MCL, MCC, MPL>(vec![]),
                PathLocal::<MCL, MCC, MPL>(vec![PathComponentLocal(vec![b'a'])]),
                PathLocal::<MCL, MCC, MPL>(vec![
                    PathComponentLocal(vec![b'a']),
                    PathComponentLocal(vec![b'b'])
                ]),
                PathLocal::<MCL, MCC, MPL>(vec![
                    PathComponentLocal(vec![b'a']),
                    PathComponentLocal(vec![b'b']),
                    PathComponentLocal(vec![b'c'])
                ]),
            ]
        )
    }

    #[test]
    fn is_prefix_of() {
        let path_a = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
        ]);

        let path_b = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(path_a.is_prefix_of(&path_b));
        assert!(!path_a.is_prefix_of(&path_c));
    }

    #[test]
    fn is_prefixed_by() {
        let path_a = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
        ]);

        let path_b = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(path_b.is_prefixed_by(&path_a));
        assert!(!path_c.is_prefixed_by(&path_a));
    }

    #[test]
    fn longest_common_prefix() {
        let path_a = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'x']),
        ]);

        let path_b = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        let lcp_a_b = path_a.longest_common_prefix(&path_b);

        assert_eq!(
            lcp_a_b,
            PathLocal::<MCL, MCC, MPL>(vec![PathComponentLocal(vec![b'a']),])
        );

        let lcp_b_a = path_b.longest_common_prefix(&path_a);

        assert_eq!(lcp_b_a, lcp_a_b);

        let lcp_a_x = path_a.longest_common_prefix(&path_c);

        assert_eq!(lcp_a_x, PathLocal::empty());
    }
}
