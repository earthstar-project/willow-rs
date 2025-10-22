use crate::prelude::*;

/// A keylike value is one that can be used to "address" [Entries](https://willowprotocol.org/specs/data-model/index.html#Entry) within a [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace).
///
/// Within a namespace, entries can be uniquely identified by their [subspace_id](https://willowprotocol.org/specs/data-model/index.html#entry_subspace_id) (of type `S`) and their [path](https://willowprotocol.org/specs/data-model/index.html#entry_path).
pub trait Keylike<const MCL: usize, const MCC: usize, const MPL: usize, S> {
    /// Returns the [subspace_id](https://willowprotocol.org/specs/data-model/index.html#entry_subspace_id) of `self`.
    fn subspace_id(&self) -> &S;

    /// Returns the [path](https://willowprotocol.org/specs/data-model/index.html#entry_path) of `self`.
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
