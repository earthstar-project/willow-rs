use std::{
    borrow::Borrow,
    ops::{Deref, DerefMut},
};

#[derive(Debug)]
pub struct ComponentTooLongError;

pub trait PathComponent: Eq + AsRef<[u8]> + Clone + PartialOrd + Ord {
    fn new(bytes: &[u8]) -> Result<Self, ComponentTooLongError>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;
}

#[derive(Debug)]
pub enum InvalidPathError {
    PathTooLong,
    TooManyComponents,
}

pub trait Path: PartialEq + Eq + PartialOrd + Ord {
    type Component: PathComponent;

    fn components(&self) -> impl Iterator<Item = &Self::Component>;

    fn count(&self) -> usize;

    fn total_length(&self) -> usize;

    fn as_prefix(&self, length: usize) -> &Self;

    fn all_prefixes(&self) -> impl Iterator<Item = &Self> {
        (0..self.count() + 1).map(|i| self.as_prefix(i))
    }

    fn is_prefix_of(&self, other: &Self) -> bool {
        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return false;
            }
        }

        true
    }

    fn is_prefixed_by(&self, other: &Self) -> bool {
        other.is_prefix_of(self)
    }

    fn longest_common_prefix(&self, other: &Self) -> &Self {
        let mut i = 0;

        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return self.as_prefix(i);
            }

            i += 1;
        }

        self.as_prefix(i)
    }
}

pub trait PathBuf:
    Deref<Target = Self::P>
    + DerefMut<Target = Self::P>
    + AsRef<Self::P>
    + Borrow<Self::P>
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Clone
    + Default
{
    type P: ?Sized + Path;

    fn from_slice(components: &[<Self::P as Path>::Component]) -> Result<Self, InvalidPathError>;

    fn from_path(path: &Self::P) -> Self;

    fn empty() -> Self;

    fn append(&mut self, component: <Self::P as Path>::Component) -> Result<(), InvalidPathError>;
}

// Single threaded default.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathComponentLocal<const MCL: usize>(Vec<u8>);

impl<const MCL: usize> PathComponent for PathComponentLocal<MCL> {
    fn new(bytes: &[u8]) -> Result<Self, ComponentTooLongError> {
        if bytes.len() > MCL {
            return Err(ComponentTooLongError);
        }

        let mut vec = Vec::new();
        vec.extend_from_slice(bytes);

        Ok(Self(vec))
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

impl<const MCL: usize> Path for [PathComponentLocal<MCL>] {
    type Component = PathComponentLocal<MCL>;

    fn components(&self) -> impl Iterator<Item = &Self::Component> {
        self.iter()
    }

    fn count(&self) -> usize {
        self.len()
    }

    fn total_length(&self) -> usize {
        self.iter().fold(0, |acc, component| acc + component.len())
    }

    fn as_prefix(&self, length: usize) -> &Self {
        let upper_bound = std::cmp::min(self.len(), length);

        &self[0..upper_bound]
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
struct PathBufLocal<const MCL: usize, const MCC: usize, const MPL: usize>(
    Vec<PathComponentLocal<MCL>>,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize> Default for PathBufLocal<MCL, MCC, MPL> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    Borrow<<PathBufLocal<MCL, MCC, MPL> as PathBuf>::P> for PathBufLocal<MCL, MCC, MPL>
{
    fn borrow(&self) -> &<PathBufLocal<MCL, MCC, MPL> as PathBuf>::P {
        self.0.borrow()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Deref for PathBufLocal<MCL, MCC, MPL> {
    type Target = <PathBufLocal<MCL, MCC, MPL> as PathBuf>::P;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> DerefMut
    for PathBufLocal<MCL, MCC, MPL>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    AsRef<<PathBufLocal<MCL, MCC, MPL> as PathBuf>::P> for PathBufLocal<MCL, MCC, MPL>
{
    fn as_ref(&self) -> &<PathBufLocal<MCL, MCC, MPL> as PathBuf>::P {
        self.0.as_ref()
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PathBuf for PathBufLocal<MCL, MCC, MPL> {
    type P = [PathComponentLocal<MCL>];

    fn from_slice(components: &[<Self::P as Path>::Component]) -> Result<Self, InvalidPathError> {
        let mut total_count = 0;
        let mut total_path_length = 0;

        for component in components {
            total_count += 1;
            total_path_length += component.len();
        }

        if total_count > MCC {
            return Err(InvalidPathError::TooManyComponents);
        }

        if total_path_length > MPL {
            return Err(InvalidPathError::PathTooLong);
        }

        let mut vec = Vec::new();

        vec.clone_from_slice(components);

        Ok(PathBufLocal(vec))
    }

    fn from_path(path: &Self::P) -> Self {
        let mut vec = Vec::new();

        vec.clone_from_slice(path);

        PathBufLocal(vec)
    }

    fn empty() -> Self {
        PathBufLocal(Vec::new())
    }

    fn append(&mut self, component: <Self::P as Path>::Component) -> Result<(), InvalidPathError> {
        if self.0.len() + 1 > MCC {
            return Err(InvalidPathError::TooManyComponents);
        }

        if self.0.total_length() > MPL {
            return Err(InvalidPathError::PathTooLong);
        }

        self.0.push(component);

        Ok(())
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
        let empty_path = PathBufLocal::<MCL, MCC, MPL>::empty();

        assert_eq!(empty_path.components().count(), 0);
    }

    #[test]
    fn new() {
        let too_many_components = PathBufLocal::<MCL, MCC, MPL>::from_slice(&[
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(matches!(
            too_many_components,
            Err(InvalidPathError::TooManyComponents)
        ));

        let path_too_long = PathBufLocal::<MCL, MCC, MPL>::from_slice(&[
            PathComponentLocal(vec![b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']),
            PathComponentLocal(vec![b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(matches!(path_too_long, Err(InvalidPathError::PathTooLong)));
    }

    #[test]
    fn append() {
        let mut path = PathBufLocal::<MCL, MCC, MPL>::empty();

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
        let path = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let prefix0 = path.as_prefix(0);

        assert_eq!(prefix0, &[]);

        let prefix1 = path.as_prefix(1);

        assert_eq!(prefix1, &[PathComponentLocal(vec![b'a'])]);

        let prefix2 = path.as_prefix(2);

        assert_eq!(
            prefix2,
            &[
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b'])
            ]
        );

        let prefix3 = path.as_prefix(3);

        assert_eq!(
            prefix3,
            &[
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b']),
                PathComponentLocal(vec![b'c'])
            ]
        );

        let prefix4 = path.as_prefix(4);

        assert_eq!(
            prefix4,
            &[
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b']),
                PathComponentLocal(vec![b'c'])
            ]
        )
    }

    #[test]
    fn prefixes() {
        let path = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let mut prefixes = path.all_prefixes();

        let p1 = prefixes.next().unwrap();
        assert_eq!(p1, &[]);

        let p2 = prefixes.next().unwrap();
        assert_eq!(p2, &[PathComponentLocal(vec![b'a'])]);

        let p3 = prefixes.next().unwrap();
        assert_eq!(
            p3,
            &[
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b'])
            ]
        );

        let p4 = prefixes.next().unwrap();
        assert_eq!(
            p4,
            &[
                PathComponentLocal(vec![b'a']),
                PathComponentLocal(vec![b'b']),
                PathComponentLocal(vec![b'c'])
            ]
        );

        assert!(prefixes.next().is_none())
    }

    #[test]
    fn is_prefix_of() {
        let path_a = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
        ]);

        let path_b = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(path_a.is_prefix_of(&path_b));
        assert!(!path_a.is_prefix_of(&path_c));
    }

    #[test]
    fn is_prefixed_by() {
        let path_a = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
        ]);

        let path_b = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        assert!(path_b.is_prefixed_by(&path_a));
        assert!(!path_c.is_prefixed_by(&path_a));
    }

    #[test]
    fn longest_common_prefix() {
        let path_a = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'x']),
        ]);

        let path_b = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'b']),
            PathComponentLocal(vec![b'c']),
        ]);

        let path_c = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'y']),
            PathComponentLocal(vec![b'z']),
        ]);

        let lcp_a_b = path_a.longest_common_prefix(&path_b);

        assert_eq!(lcp_a_b, &[PathComponentLocal(vec![b'a']),]);

        let lcp_b_a = path_b.longest_common_prefix(&path_a);

        assert_eq!(lcp_b_a, lcp_a_b);

        let lcp_a_x = path_a.longest_common_prefix(&path_c);

        assert_eq!(lcp_a_x, &[]);

        let path_d = PathBufLocal::<MCL, MCC, MPL>(vec![
            PathComponentLocal(vec![b'a']),
            PathComponentLocal(vec![b'x']),
            PathComponentLocal(vec![b'c']),
        ]);

        let lcp_b_d = path_b.longest_common_prefix(&path_d);

        assert_eq!(lcp_b_d, &[PathComponentLocal(vec![b'a']),])
    }
}
