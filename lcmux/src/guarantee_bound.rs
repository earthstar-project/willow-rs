use core::cell::Cell;

/// Tracks guarantees issued by the other peer.
#[derive(Debug)]
pub(crate) struct GuaranteeBoundCell(Cell<Option<u64>>);

impl GuaranteeBoundCell {
    /// Creates a new instance with no bound yet set.
    pub fn new() -> Self {
        Self(Cell::new(None))
    }

    pub fn get_bound(&self) -> Option<u64> {
        self.0.get()
    }

    /// Tries to apply a new bound to this: if non was set before, or if the new bound is tighter than the old one, returns `Ok(())`. Otherwise, returns `Err(old_bound)`.
    pub fn apply_bound(&self, new_bound: u64) -> Result<(), u64> {
        match self.0.get() {
            None => {
                self.0.set(Some(new_bound));
                Ok(())
            }
            Some(old_bound) => {
                if new_bound <= old_bound {
                    self.0.set(Some(new_bound));
                    Ok(())
                } else {
                    Err(old_bound)
                }
            }
        }
    }

    /// Uses up some of the budget, updating the bound and reporting if it is exceeded. Reports `Ok(None)` if no bound was set, `Ok(remaining_guarantees)` if a bound was set and not violated, or `Err(())` if using this many guarantees exceeds the bound.
    pub fn use_up(&self, guarantees: u64) -> Result<Option<u64>, ()> {
        match self.0.get() {
            None => Ok(None),
            Some(old_bound) => match old_bound.checked_sub(guarantees) {
                Some(new_bound) => {
                    self.0.set(Some(new_bound));
                    Ok(Some(new_bound))
                }
                None => Err(()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_bound() {
        let bound = GuaranteeBoundCell::new();

        assert_eq!(Ok(()), bound.apply_bound(123));
        assert_eq!(Ok(()), bound.apply_bound(122));
        assert_eq!(Err(122), bound.apply_bound(124));
    }

    #[test]
    fn test_use() {
        let bound = GuaranteeBoundCell::new();

        assert_eq!(Ok(None), bound.use_up(17));
        assert_eq!(Ok(()), bound.apply_bound(123));
        assert_eq!(Ok(Some(113)), bound.use_up(10));
        assert_eq!(Err(()), bound.use_up(114));
    }

    #[test]
    fn test_use_exact() {
        let bound = GuaranteeBoundCell::new();

        assert_eq!(Ok(()), bound.apply_bound(123));
        assert_eq!(Ok(Some(0)), bound.use_up(123));
    }
}
