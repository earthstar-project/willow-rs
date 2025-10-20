use crate::prelude::*;

/// A keylike value is one that can be used to "address" entries within a namespace, i.e., a type which specifies a SubspaceId and a Path.
pub trait Keylike<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// Returns the SubspaceId of `self`.
    fn subspace_id(&self) -> &S;

    /// Returns the Path of `self`.
    fn path(&self) -> &Path<MCL, MCC, MPL>;
}

impl<const MCL: usize, const MCC: usize, const MPL: usize, S> Keylike<MCL, MCC, MPL, S>
    for (S, Path<MCL, MCC, MPL>)
{
    fn subspace_id(&self) -> &S {
        &self.0
    }

    fn path(&self) -> &Path<MCL, MCC, MPL> {
        &self.1
    }
}
