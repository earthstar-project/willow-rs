use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use libfuzzer_sys::arbitrary::{self, Arbitrary, Error as ArbitraryError, Unstructured};

use willow_data_model_generic::prelude::*;
/*
* A known-good, simple implementation of paths. Used in testing to compare the behaviour of the optimised implementation against it.
*/

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathComponentBox<const MCL: usize>(Box<[u8]>);

/// An implementation of [`PathComponent`] for [`PathComponentBox`].
///
/// ## Type parameters
///
/// - `MCL`: A [`usize`] used as [`PathComponent::MAX_COMPONENT_LENGTH`].
impl<const MCL: usize> PathComponentBox<MCL> {
    const MAX_COMPONENT_LENGTH: usize = MCL;

    /// Create a new component by cloning and appending all bytes from the slice into a [`Vec<u8>`], or return a [`InvalidComponentError`] if the bytelength of the slice exceeds [`PathComponent::MAX_COMPONENT_LENGTH`].
    pub fn new(bytes: &[u8]) -> Result<Self, InvalidComponentError> {
        if bytes.len() > Self::MAX_COMPONENT_LENGTH {
            return Err(InvalidComponentError);
        }

        Ok(Self(bytes.into()))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Construct a new [`PathComponent`] from the concatenation of `head` and the `tail`, or return a [`InvalidComponentError`] if the resulting component would be longer than [`PathComponent::MAX_COMPONENT_LENGTH`].
    ///
    /// This operation occurs when computing prefix successors, and the default implementation needs to perform an allocation. Implementers of this trait can override this with something more efficient if possible.
    pub fn new_with_tail(head: &[u8], tail: u8) -> Result<Self, InvalidComponentError> {
        let mut vec = Vec::with_capacity(head.len() + 1);
        vec.extend_from_slice(head);
        vec.push(tail);

        Self::new(&vec)
    }

    /// Return a new [`PathComponent`] which corresponds to the empty string.
    pub fn new_empty() -> Self {
        Self::new(&[]).unwrap()
    }

    /// Try to append a zero byte to the end of the component.
    /// Return `None` if the resulting component would be too long.
    fn try_append_zero_byte(&self) -> Option<Self> {
        if self.len() == Self::MAX_COMPONENT_LENGTH {
            return None;
        }

        let mut new_component_vec = Vec::with_capacity(self.len() + 1);

        new_component_vec.extend_from_slice(self.as_ref());
        new_component_vec.push(0);

        Some(Self::new(&new_component_vec).unwrap())
    }

    fn set_byte(&self, i: usize, value: u8) -> Self {
        let mut new_component = self.clone();

        new_component.0[i] = value;

        new_component
    }

    /// Interpret the component as a binary number, and increment that number by 1. When a byte overflows, remove it.
    /// If doing so would increase the bytelength of the component, return `None`.
    fn try_increment_fixed_width_dropping_trailings(&self) -> Option<Self> {
        let mut new_component = self.clone();

        for i in (0..self.len()).rev() {
            let byte = self.as_ref()[i];

            if byte == 255 {
                new_component = new_component.set_byte(i, 0);

                if new_component.len() > 1 {
                    new_component =
                        PathComponentBox::new(&new_component.as_ref()[..new_component.len() - 1])
                            .unwrap();
                } else {
                    return None;
                }
            } else {
                return Some(new_component.set_byte(i, byte + 1));
            }
        }

        None
    }

    /// Return the least component which is greater than `self` but which is not prefixed by `self`.
    fn greater_but_not_prefixed(&self) -> Option<Self> {
        for i in (0..self.len()).rev() {
            if self.as_ref()[i] != 255 {
                // Since we are not adjusting the length of the component this will always succeed.
                return Some(
                    Self::new_with_tail(&self.as_ref()[0..i], self.as_ref()[i] + 1).unwrap(),
                );
            }
        }

        None
    }
}

impl<const MCL: usize> AsRef<[u8]> for PathComponentBox<MCL> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'a, const MCL: usize> Arbitrary<'a> for PathComponentBox<MCL> {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, ArbitraryError> {
        let boxx: Box<[u8]> = Arbitrary::arbitrary(u)?;
        Self::new(&boxx).map_err(|_| ArbitraryError::IncorrectFormat)
    }

    #[inline]
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        <Box<[u8]> as Arbitrary<'a>>::size_hint(depth)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
/// A cheaply cloneable [`Path`] using a `Rc<[PathComponentBox]>`.
/// While cloning is cheap, operations which return modified forms of the path (e.g. [`Path::append`]) are not, as they have to clone and adjust the contents of the underlying [`Rc`].
pub struct PathRc<const MCL: usize, const MCC: usize, const MPL: usize>(
    Rc<[PathComponentBox<MCL>]>,
);

impl<const MCL: usize, const MCC: usize, const MPL: usize> PathRc<MCL, MCC, MPL> {
    pub fn new() -> Self {
        PathRc(Vec::new().into())
    }

    pub fn from_component(comp: PathComponentBox<MCL>) -> Result<Self, PathFromComponentsError> {
        Self::from_components(&[comp])
    }

    pub fn from_slice<T: AsRef<[u8]>>(comp: T) -> Result<Self, PathError> {
        Self::from_slices(&[comp])
    }

    pub fn from_components(
        components: &[PathComponentBox<MCL>],
    ) -> Result<Self, PathFromComponentsError> {
        if components.len() > MCC {
            return Err(PathFromComponentsError::TooManyComponents);
        };

        let mut path_vec = Vec::new();
        let mut total_length = 0;

        for component in components {
            total_length += component.len();

            if total_length > MPL {
                return Err(PathFromComponentsError::PathTooLong);
            } else {
                path_vec.push(component.clone());
            }
        }

        Ok(PathRc(path_vec.into()))
    }

    pub fn from_slices<T: AsRef<[u8]>>(slices: &[T]) -> Result<Self, PathError> {
        if slices.len() > MCC {
            return Err(PathError::TooManyComponents);
        };

        let mut path_vec = Vec::new();
        let mut total_length = 0;

        for component in slices {
            if component.as_ref().len() > MCL {
                return Err(PathError::ComponentTooLong);
            }
            total_length += component.as_ref().len();

            if total_length > MPL {
                return Err(PathError::PathTooLong);
            } else {
                path_vec.push(PathComponentBox::new(component.as_ref())?);
            }
        }

        Ok(PathRc(path_vec.into()))
    }

    pub fn from_components_iter<I>(
        _total_length: usize,
        iter: &mut I,
    ) -> Result<Self, PathFromComponentsError>
    where
        I: ExactSizeIterator<Item = PathComponentBox<MCL>>,
    {
        let components: Vec<_> = iter.collect();

        Self::from_components(&components[..])
    }

    pub fn from_slices_iter<'a, I, T>(_total_length: usize, iter: &mut I) -> Result<Self, PathError>
    where
        I: ExactSizeIterator<Item = T>,
        T: AsRef<[u8]>,
    {
        let components: Vec<_> = iter.collect();

        Self::from_slices(&components[..])
    }

    pub fn append_component(
        &self,
        comp: PathComponentBox<MCL>,
    ) -> Result<Self, PathFromComponentsError> {
        let total_component_count = self.0.len();

        if total_component_count + 1 > MCC {
            return Err(PathFromComponentsError::TooManyComponents);
        }

        let total_path_length = self.0.iter().fold(0, |acc, item| acc + item.0.len());

        if total_path_length + comp.as_ref().len() > MPL {
            return Err(PathFromComponentsError::PathTooLong);
        }

        let mut new_path_vec = Vec::new();

        for component in self.components() {
            new_path_vec.push(component.clone())
        }

        new_path_vec.push(comp);

        Ok(PathRc(new_path_vec.into()))
    }

    pub fn append_slice<T: AsRef<[u8]>>(&self, comp: T) -> Result<Self, PathError> {
        let component = PathComponentBox::new(comp.as_ref())?;
        Ok(self.append_component(component)?)
    }

    pub fn append_components(
        &self,
        components: &[PathComponentBox<MCL>],
    ) -> Result<Self, PathFromComponentsError> {
        let mut p = self.clone();

        for component in components {
            p = p.append_component(component.clone())?;
        }

        Ok(p)
    }

    pub fn append_slices<T: AsRef<[u8]>>(&self, components: &[T]) -> Result<Self, PathError> {
        let mut p = self.clone();

        for component in components {
            p = p.append_slice(component)?;
        }

        Ok(p)
    }

    pub fn append_path(&self, other: &Self) -> Result<Self, PathFromComponentsError> {
        let mut p = self.clone();

        for component in other.components() {
            p = p.append_component(component.clone())?;
        }

        Ok(p)
    }

    pub fn greater_but_not_prefixed(&self) -> Option<Self> {
        for (i, component) in self.components().enumerate().rev() {
            if let Some(successor_comp) = component.try_append_zero_byte() {
                if let Ok(path) = self
                    .create_prefix(i)
                    .unwrap()
                    .append_component(successor_comp)
                {
                    return Some(path);
                }
            }

            if let Some(successor_comp) = component.greater_but_not_prefixed() {
                return self
                    .create_prefix(i)
                    .unwrap()
                    .append_component(successor_comp)
                    .ok();
            }
        }

        None
    }

    pub fn component_count(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.component_count() == 0
    }

    pub fn total_length(&self) -> usize {
        self.components().fold(0, |acc, x| acc + x.len())
    }

    pub fn total_length_of_prefix(&self, i: usize) -> usize {
        if i > self.component_count() {
            panic!();
        }

        self.create_prefix(i)
            .unwrap()
            .components()
            .fold(0, |acc, x| acc + x.len())
    }

    /// Test whether this path is a prefix of the given path.
    /// Paths are always a prefix of themselves.
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                return false;
            }
        }

        self.component_count() <= other.component_count()
    }

    /// Test whether this path is prefixed by the given path.
    /// Paths are always a prefix of themselves.
    pub fn is_prefixed_by(&self, other: &Self) -> bool {
        other.is_prefix_of(self)
    }

    pub fn is_related_to(&self, other: &Self) -> bool {
        self.is_prefix_of(other) || other.is_prefix_of(self)
    }

    pub fn component(&self, i: usize) -> Option<&PathComponentBox<MCL>> {
        self.0.get(i)
    }

    pub fn component_unchecked(&self, i: usize) -> &PathComponentBox<MCL> {
        self.component(i).unwrap()
    }

    pub fn components(
        &self,
    ) -> impl DoubleEndedIterator<Item = &PathComponentBox<MCL>>
           + ExactSizeIterator<Item = &PathComponentBox<MCL>> {
        self.0.iter()
    }

    pub fn suffix_components(
        &self,
        i: usize,
    ) -> impl DoubleEndedIterator<Item = &PathComponentBox<MCL>>
           + ExactSizeIterator<Item = &PathComponentBox<MCL>> {
        self.components().skip(i)
    }

    pub fn create_prefix(&self, length: usize) -> Option<Self> {
        if length > self.component_count() {
            return None;
        }

        if length == 0 {
            return Some(Self::new());
        }

        let until = core::cmp::min(length, self.0.len());
        let slice = &self.0[0..until];

        Some(Self::from_components(slice).unwrap())
    }

    pub unsafe fn create_prefix_unchecked(&self, component_count: usize) -> Self {
        self.create_prefix(component_count).unwrap()
    }

    /// Return all possible prefixes of a path, including the empty path and the path itself.
    pub fn all_prefixes(&self) -> impl Iterator<Item = Self> + '_ {
        let self_len = self.components().count();

        (0..=self_len).map(move |i| self.create_prefix(i).unwrap())
    }

    /// Return the longest common prefix of this path and the given path.
    pub fn longest_common_prefix(&self, other: &Self) -> Self {
        let mut lcp_len = 0;

        for (comp_a, comp_b) in self.components().zip(other.components()) {
            if comp_a != comp_b {
                break;
            }

            lcp_len += 1
        }

        self.create_prefix(lcp_len).unwrap()
    }

    /// Return the least path which is greater than `self`, or return `None` if `self` is the greatest possible path.
    pub fn successor(&self) -> Option<Self> {
        // Try and add an empty component.
        if let Ok(path) = self.append_component(PathComponentBox::<MCL>::new_empty()) {
            return Some(path);
        }

        for (i, component) in self.components().enumerate().rev() {
            // Try and do the *next* simplest thing (add a 0 byte to the component).
            if let Some(component) = component.try_append_zero_byte() {
                if let Ok(path) = self.create_prefix(i).unwrap().append_component(component) {
                    return Some(path);
                }
            }

            // Otherwise we need to increment the component fixed-width style!
            if let Some(incremented_component) =
                component.try_increment_fixed_width_dropping_trailings()
            {
                // We can unwrap here because neither the max path length, component count, or component length has changed.
                return Some(
                    self.create_prefix(i)
                        .unwrap()
                        .append_component(incremented_component)
                        .unwrap(),
                );
            }
        }

        None
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Ord for PathRc<MCL, MCC, MPL> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for (my_comp, your_comp) in self.components().zip(other.components()) {
            let comparison = my_comp.cmp(your_comp);

            match comparison {
                std::cmp::Ordering::Equal => { /* Continue */ }
                _ => return comparison,
            }
        }

        self.component_count().cmp(&other.component_count())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> PartialOrd for PathRc<MCL, MCC, MPL> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for PathRc<MCL, MCC, MPL>
{
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, ArbitraryError> {
        let boxx: Box<[PathComponentBox<MCL>]> = Arbitrary::arbitrary(u)?;
        Self::from_components(&boxx).map_err(|_| ArbitraryError::IncorrectFormat)
    }

    #[inline]
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        <Box<[u8]> as Arbitrary<'a>>::size_hint(depth)
    }
}

/*
Instructions for how to create paths; for fuzz testing.
*/

#[derive(Debug, Arbitrary)]
pub enum CreatePath {
    Empty,
    FromComponent(Vec<u8>),
    FromSlice(Vec<u8>),
    FromComponents(Vec<Vec<u8>>),
    FromSlices(Vec<Vec<u8>>),
    FromComponentsIter(Vec<Vec<u8>>),
    FromSlicesIter(Vec<Vec<u8>>),
    AppendComponent(Box<CreatePath>, Vec<u8>),
    AppendSlice(Box<CreatePath>, Vec<u8>),
    AppendComponents(Box<CreatePath>, Vec<Vec<u8>>),
    AppendSlices(Box<CreatePath>, Vec<Vec<u8>>),
    AppendPath(Box<CreatePath>, Box<CreatePath>),
    GreaterButNotPrefixed(Box<CreatePath>),
    Successor(Box<CreatePath>),
    CreatePrefix(Box<CreatePath>, usize),
}

// When a construction is invalid for reasons other than a PathError, simply return `Err(PathError::PathTooLong)`.
pub fn create_path_rc<const MCL: usize, const MCC: usize, const MPL: usize>(
    cp: &CreatePath,
) -> Result<PathRc<MCL, MCC, MPL>, PathError> {
    match cp {
        CreatePath::Empty => Ok(PathRc::new()),
        CreatePath::FromComponent(comp) => {
            Ok(PathRc::from_component(PathComponentBox::new(&comp[..])?)?)
        }
        CreatePath::FromSlice(comp) => PathRc::from_slice(&comp[..]),
        CreatePath::FromComponents(raw_material) => {
            let mut components = vec![];

            for comp in raw_material {
                components.push(PathComponentBox::new(&comp[..])?);
            }

            Ok(PathRc::from_components(&components[..])?)
        }
        CreatePath::FromSlices(raw_material) => {
            let mut components = vec![];

            for comp in raw_material {
                components.push(&comp[..]);
            }

            PathRc::from_slices(&components[..])
        }
        CreatePath::FromComponentsIter(raw_material) => {
            let mut components = vec![];
            let mut total_length = 0;

            for comp in raw_material {
                components.push(PathComponentBox::new(&comp[..])?);
                total_length += comp.len();
            }

            Ok(PathRc::from_components_iter(
                total_length,
                &mut components.into_iter(),
            )?)
        }
        CreatePath::FromSlicesIter(raw_material) => {
            let mut components = vec![];
            let mut total_length = 0;

            for comp in raw_material {
                components.push(&comp[..]);
                total_length += comp.len();
            }

            PathRc::from_slices_iter(total_length, &mut components.into_iter())
        }
        CreatePath::AppendComponent(rec, comp) => {
            let base = create_path_rc(rec)?;
            Ok(base.append_component(PathComponentBox::new(comp)?)?)
        }
        CreatePath::AppendSlice(rec, comp) => {
            let base = create_path_rc(rec)?;
            base.append_slice(PathComponentBox::<MCL>::new(&comp[..])?)
        }
        CreatePath::AppendComponents(rec, raw_material) => {
            let base = create_path_rc(rec)?;

            let mut components = vec![];

            for comp in raw_material {
                components.push(PathComponentBox::new(&comp[..])?);
            }

            Ok(base.append_components(&components[..])?)
        }
        CreatePath::AppendSlices(rec, raw_material) => {
            let base = create_path_rc(rec)?;

            let mut components = vec![];

            for comp in raw_material {
                components.push(&comp[..]);
            }

            base.append_slices(&components[..])
        }
        CreatePath::AppendPath(rec, p2) => {
            let base = create_path_rc(rec)?;
            let p2 = create_path_rc(p2)?;
            Ok(base.append_path(&p2)?)
        }
        CreatePath::GreaterButNotPrefixed(rec) => {
            let base = create_path_rc(rec)?;
            base.greater_but_not_prefixed()
                .ok_or(PathError::PathTooLong)
        }
        CreatePath::Successor(rec) => {
            let base = create_path_rc(rec)?;
            base.successor().ok_or(PathError::PathTooLong)
        }
        CreatePath::CreatePrefix(rec, len) => {
            let base = create_path_rc(rec)?;

            if *len > base.component_count() {
                Err(PathError::PathTooLong)
            } else {
                Ok(base.create_prefix(*len).unwrap())
            }
        }
    }
}

pub fn create_path<const MCL: usize, const MCC: usize, const MPL: usize>(
    cp: &CreatePath,
) -> Result<Path<MCL, MCC, MPL>, PathError> {
    match cp {
        CreatePath::Empty => Ok(Path::new()),
        CreatePath::FromComponent(comp) => Ok(Path::from_component(Component::new(&comp[..])?)?),
        CreatePath::FromSlice(comp) => Path::from_slice(&comp[..]),
        CreatePath::FromComponents(raw_material) => {
            let mut components = vec![];

            for comp in raw_material {
                components.push(Component::new(&comp[..])?);
            }

            Ok(Path::from_components(&components[..])?)
        }
        CreatePath::FromSlices(raw_material) => {
            let mut components = vec![];

            for comp in raw_material {
                components.push(&comp[..]);
            }

            Path::from_slices(&components[..])
        }
        CreatePath::FromComponentsIter(raw_material) => {
            let mut components = vec![];
            let mut total_length = 0;

            for comp in raw_material {
                components.push(Component::new(&comp[..])?);
                total_length += comp.len();
            }

            Ok(Path::from_components_iter(
                total_length,
                &mut components.into_iter(),
            )?)
        }
        CreatePath::FromSlicesIter(raw_material) => {
            let mut components = vec![];
            let mut total_length = 0;

            for comp in raw_material {
                components.push(&comp[..]);
                total_length += comp.len();
            }

            Path::from_slices_iter(total_length, &mut components.into_iter())
        }
        CreatePath::AppendComponent(rec, comp) => {
            let base = create_path(rec)?;
            Ok(base.append_component(Component::new(comp)?)?)
        }
        CreatePath::AppendSlice(rec, comp) => {
            let base = create_path(rec)?;
            base.append_slice(Component::<MCL>::new(&comp[..])?)
        }
        CreatePath::AppendComponents(rec, raw_material) => {
            let base = create_path(rec)?;

            let mut components = vec![];

            for comp in raw_material {
                components.push(Component::new(&comp[..])?);
            }

            Ok(base.append_components(&components[..])?)
        }
        CreatePath::AppendSlices(rec, raw_material) => {
            let base = create_path(rec)?;

            let mut components = vec![];

            for comp in raw_material {
                components.push(&comp[..]);
            }

            base.append_slices(&components[..])
        }
        CreatePath::AppendPath(rec, p2) => {
            let base = create_path(rec)?;
            let p2 = create_path(p2)?;
            Ok(base.append_path(&p2)?)
        }
        CreatePath::GreaterButNotPrefixed(rec) => {
            let base = create_path(rec)?;
            base.greater_but_not_prefixed()
                .ok_or(PathError::PathTooLong)
        }
        CreatePath::Successor(rec) => {
            let base = create_path(rec)?;
            base.successor().ok_or(PathError::PathTooLong)
        }
        CreatePath::CreatePrefix(rec, len) => {
            let base = create_path(rec)?;

            if *len > base.component_count() {
                Err(PathError::PathTooLong)
            } else {
                Ok(base.create_prefix(*len).unwrap())
            }
        }
    }
}

/*
Check that the two `Path`s behave just like `PathRc`s. For fuzz testing.
*/
pub fn assert_isomorphic_paths<const MCL: usize, const MCC: usize, const MPL: usize>(
    ctrl1: &PathRc<MCL, MCC, MPL>,
    ctrl2: &PathRc<MCL, MCC, MPL>,
    p1: &Path<MCL, MCC, MPL>,
    p2: &Path<MCL, MCC, MPL>,
) {
    assert_eq!(ctrl1.component_count(), p1.component_count());

    assert_eq!(ctrl1.is_empty(), p1.is_empty());

    assert_eq!(
        ctrl1.total_length(),
        p1.total_length(),
        "control: {:?}\nactual: {:?}",
        ctrl1,
        p1
    );

    for i in 0..=ctrl1.component_count() {
        assert_eq!(
            ctrl1.total_length_of_prefix(i),
            p1.total_length_of_prefix(i)
        );
    }

    assert_eq!(ctrl1.is_prefix_of(ctrl2), p1.is_prefix_of(p2));
    assert_eq!(ctrl1.is_prefixed_by(ctrl2), p1.is_prefixed_by(p2));
    assert_eq!(ctrl1.is_related_to(ctrl2), p1.is_related_to(p2));

    for i in 0..ctrl1.component_count() {
        assert_eq!(
            ctrl1.component(i).map(|comp| comp.as_ref()),
            p1.component(i).map(|comp| comp.as_ref())
        );

        unsafe {
            assert_eq!(
                ctrl1.component_unchecked(i).as_ref(),
                p1.component_unchecked(i).as_ref(),
            );
        }

        assert_eq!(
            ctrl1.component(i).unwrap().as_ref(),
            p1.owned_component(i).unwrap().as_ref()
        );

        unsafe {
            assert_eq!(
                ctrl1.component(i).unwrap().as_ref(),
                p1.owned_component_unchecked(i).as_ref()
            );
        }
    }

    assert!(p1.component(p1.component_count()).is_none());
    assert!(p1.owned_component(p1.component_count()).is_none());

    assert!(ctrl1
        .components()
        .map(|comp| comp.as_ref())
        .eq(p1.components().map(|comp| comp.as_ref())));

    for i in 0..=ctrl1.component_count() {
        assert_eq!(
            ctrl1.suffix_components(i).count(),
            p1.suffix_components(i).count()
        );

        ctrl1
            .suffix_components(i)
            .map(|comp| comp.as_ref())
            .eq(p1.suffix_components(i).map(|comp| comp.as_ref()));
    }

    assert!(ctrl1
        .components()
        .map(|comp| Box::<[u8]>::from(comp.as_ref()))
        .eq(p1
            .owned_components()
            .map(|comp| Box::<[u8]>::from(comp.as_ref()))));

    for i in 0..=ctrl1.component_count() {
        assert_eq!(
            ctrl1.suffix_components(i).count(),
            p1.suffix_owned_components(i).count()
        );

        ctrl1
            .suffix_components(i)
            .map(|comp| Box::<[u8]>::from(comp.as_ref()))
            .eq(p1
                .suffix_owned_components(i)
                .map(|comp| Box::<[u8]>::from(comp.as_ref())));
    }

    for i in 0..=ctrl1.component_count() {
        assert_paths_are_equal(
            &ctrl1.create_prefix(i).unwrap(),
            &p1.create_prefix(i).unwrap(),
        );

        unsafe {
            assert_paths_are_equal(
                &ctrl1.create_prefix_unchecked(i),
                &p1.create_prefix_unchecked(i),
            );
        }
    }
    assert!(p1.create_prefix(p1.component_count() + 1).is_none());

    assert_eq!(ctrl1.all_prefixes().count(), p1.all_prefixes().count());
    for (ctrl_prefix, p_prefix) in ctrl1.all_prefixes().zip(p1.all_prefixes()) {
        assert_paths_are_equal(&ctrl_prefix, &p_prefix);
    }

    let ctrl_lcp = ctrl1.longest_common_prefix(ctrl2);
    let p_lcp = p1.longest_common_prefix(p2);
    assert_paths_are_equal(&ctrl_lcp, &p_lcp);

    assert_eq!(ctrl1 == ctrl2, p1 == p2);

    if ctrl1 == ctrl2 {
        let mut h1 = DefaultHasher::new();
        p1.hash(&mut h1);
        let digest1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        p1.hash(&mut h2);
        let digest2 = h2.finish();

        assert_eq!(digest1, digest2);
    }

    assert_eq!(ctrl1.partial_cmp(ctrl2), p1.partial_cmp(p2));
    assert_eq!(ctrl1.cmp(ctrl2), p1.cmp(p2));
}

fn assert_paths_are_equal<const MCL: usize, const MCC: usize, const MPL: usize>(
    ctrl: &PathRc<MCL, MCC, MPL>,
    p: &Path<MCL, MCC, MPL>,
) {
    assert!(
        ctrl.components()
            .map(|comp| comp.as_ref())
            .eq(p.components().map(|comp| comp.as_ref())),
        "Unequal paths.\nctrl: {:?}\np: {:?}",
        ctrl,
        p,
    );
}

/*
* Utilities for testing the correctness of path successor and path prefix successors via fuzz testing.
*/

pub fn test_successor<const MCL: usize, const MCC: usize, const MPL: usize>(
    // A path whose successor was computed.
    baseline: PathRc<MCL, MCC, MPL>,
    // The computed successor of the baseline.
    candidate: PathRc<MCL, MCC, MPL>,
    // The maximal path for the given choice of MCL, MCC, MPL (we could compute it, but we were too lazy to implement that).
    max_path: PathRc<MCL, MCC, MPL>,
) {
    let successor = baseline.successor();

    match successor {
        None => {
            if baseline != max_path {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {successor:?}");
                println!("candidate: {candidate:?}");
                println!("\n\n\n");
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo")
            }
        }
        Some(successor) => {
            if successor <= baseline {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {successor:?}");
                println!("candidate: {candidate:?}");
                println!("\n\n\n");

                panic!("successor was not greater than the path it was derived from! BooooOoooOOo")
            }

            if candidate < successor && candidate > baseline {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {successor:?}");
                println!("candidate: {candidate:?}");
                println!("\n\n\n");

                panic!("the successor generated was NOT the immediate successor! BooooOOOOo!")
            }
        }
    }
}

pub fn test_greater_but_not_prefixed<const MCL: usize, const MCC: usize, const MPL: usize>(
    baseline: PathRc<MCL, MCC, MPL>,
    candidate: PathRc<MCL, MCC, MPL>,
    unsucceedable: &[PathRc<MCL, MCC, MPL>],
) {
    let greater_but_not_prefixed = baseline.greater_but_not_prefixed();

    match greater_but_not_prefixed {
        None => {
            if !unsucceedable.iter().any(|unsuc| unsuc == &baseline) {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {greater_but_not_prefixed:?}");
                println!("candidate: {candidate:?}");
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo\n\n\n\n");
            }
        }
        Some(greater_but_not_prefixed) => {
            if greater_but_not_prefixed <= baseline {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {greater_but_not_prefixed:?}");
                println!("candidate: {candidate:?}");
                panic!(
                    "the successor is meant to be greater than the baseline, but wasn't!! BOOOOOOOOO\n\n\n\n"
                );
            }

            if greater_but_not_prefixed.is_prefixed_by(&baseline) {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {greater_but_not_prefixed:?}");
                println!("candidate: {candidate:?}");
                panic!(
                    "successor was prefixed by the path it was derived from! BoooOOooOOooOo\n\n\n\n"
                );
            }

            if !baseline.is_prefix_of(&candidate)
                && candidate < greater_but_not_prefixed
                && candidate > baseline
            {
                println!("\n\n\n");
                println!("baseline: {baseline:?}");
                println!("successor: {greater_but_not_prefixed:?}");
                println!("candidate: {candidate:?}");

                panic!(
                    "the successor generated was NOT the immediate prefix successor! BooooOOOOo!\n\n\n\n"
                );
            }
        }
    }
}
