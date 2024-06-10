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

pub trait Path: PartialEq + Eq + PartialOrd + Ord {
    type Component: Eq + AsRef<[u8]> + Clone;

    const MAX_COMPONENT_LENGTH: usize;
    const MAX_COMPONENT_COUNT: usize;
    const MAX_PATH_LENGTH: usize;

    fn empty() -> Self;

    fn append(&mut self, component: Self::Component) -> Result<(), AppendPathError>;

    fn prefix(&self, length: usize) -> Self;

    fn is_prefix_of(&self, other: &Self) -> bool;

    fn is_prefixed_by(&self, other: &Self) -> bool;

    fn components(&self) -> impl Iterator<Item = &Self::Component>;
}

pub fn path_eq<P: Path>(a: &P, b: &P) -> bool {
    a.components().eq(b.components())
}

pub fn lowest_common_prefix<P: Path>(a: &P, b: &P) -> P {
    let mut new_path: P = Path::empty();

    a.components()
        .zip(b.components())
        .for_each(|(a_comp, b_comp)| {
            if a_comp == b_comp {
                new_path.append(a_comp.clone()).unwrap();
            }
        });

    new_path
}

// Implement fns that can do partialeq, eq, etc, which people can use for their Path implementations

pub fn validate<P: Path>(path: &P) -> Result<&P, InvalidPathError> {
    if path.components().count() > P::MAX_COMPONENT_COUNT {
        return Err(InvalidPathError::TooManyComponents);
    }

    let mut total_length = 0;

    for (i, component) in path.components().enumerate() {
        let length = component.as_ref().len();
        if length > P::MAX_COMPONENT_LENGTH {
            return Err(InvalidPathError::ComponentTooLong(i));
        }
        total_length += length;
    }

    if total_length > P::MAX_PATH_LENGTH {
        return Err(InvalidPathError::PathTooLong);
    }

    Ok(path)
}
