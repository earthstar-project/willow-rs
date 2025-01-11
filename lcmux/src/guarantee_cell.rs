//! A threshold shelf is like a [`wb_async_utils::shelf`] of `u64`s, except:
//!
//! - The sender does not set absolute values but that overwrite the old value, but increases a stored value by a specified amount.
//! - The receiver gets notified only once the accumulated amount reaches a threshold that it specified.
//! - After being notified of the threshold being reached, the accumulated value is reduced by either the threshold or down to zero, depending on which method was used to wait for it being reached.
//! - It also incorporates a `GuaranteeBound` to allow for setting and tracking voluntary limits on how many more guarantees will be worked with. Waiting for a threshold that cannot possibly be reached due to a bound emits an immediate error notification.

use core::cell::Cell;

use either::Either::{self, *};

use wb_async_utils::TakeCell;

use crate::guarantee_bound::GuaranteeBoundCell;

/// Manages guarantees via interior mutability. Tracks available guarantees, allows subscribing to a certain amount to become available, and tracks volunary bounds on the guarantees. Adding more guarantees subtracts them from the current bound, if any, and reports an error if that violates the bound.
#[derive(Debug)]
pub(crate) struct GuaranteeCell {
    acc: Cell<u64>,
    threshold: Cell<Option<u64>>,
    notify: TakeCell<ThresholdOutcome>,
    bound: GuaranteeBoundCell,
}

/// The possible outcomes of asking for a certain amount of guarantes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ThresholdOutcome {
    YayReached, // The guarantees became available. The threshold shelf assumes them to be used up now.
    NopeBound,  // The threshold will never be reached, because of a guarantee bound.
}

impl GuaranteeCell {
    /// Creates a new [`GuaranteeCell`]. Accumulated value is zero, no threshold is registered, no bound is set.
    pub fn new() -> Self {
        GuaranteeCell {
            acc: Cell::new(0),
            threshold: Cell::new(None),
            notify: TakeCell::new(),
            bound: GuaranteeBoundCell::new(),
        }
    }

    /// Tries to apply a new bound to this: if none was set before, or if the new bound is tighter than the old one, returns `Ok(())`. Otherwise, returns `Err(old_bound)`.
    pub fn apply_bound(&self, new_bound: u64) -> Result<(), u64> {
        let _ = self.bound.apply_bound(new_bound)?;

        // Notify waiting task if the bound made a current threshold unreachable.
        self.notify_threshold();

        Ok(())
    }

    /// Error if the guarantees would overflow our pool of guarantees, or exceed a prior bound.
    pub fn add_guarantees(&self, amount: u64) -> Result<(), ()> {
        // Can we increase the guarantees without an overflow?
        match self.acc.get().checked_add(amount) {
            // Nope, report an error.
            None => Err(()),
            Some(new_amount) => {
                // Yes, so update guarantees.
                self.acc.set(new_amount);

                // Update bound, reporting errors if needed (we might add too many guarantees and violate the bound).
                let _ = self.bound.use_up(amount)?;

                // Notify any waiting task if possible/necessary.
                self.notify_threshold();

                Ok(())
            }
        }
    }

    /// Reduces the stored guarantees down to `target`. If `target` is greater than or equal to the current amount, does nothing. Returns how many guarantees were removed (which is zero if target is gtreater than or equal to the current amount).
    pub fn reduce_guarantees_down_to(&self, target: u64) -> u64 {
        let diff = self.acc.get().saturating_sub(target);
        self.acc.set(target);
        diff
    }

    /// Park the task until the given threshold has been reached, then reduce self.acc by the threshold and return ThresholdOutcome::YayReached. If at any point the threshold is known to be unreachable, notifies the task with ThresholdOutcome::NopeBound (and does not touch self.acc).
    pub async fn await_threshold_sub(&self, threshold: u64) -> ThresholdOutcome {
        self.threshold.set(Some(threshold));
        self.notify_threshold();
        match self.notify.take().await {
            ThresholdOutcome::YayReached => {
                self.acc.set(self.acc.get() - threshold);
                ThresholdOutcome::YayReached
            }
            ThresholdOutcome::NopeBound => ThresholdOutcome::NopeBound,
        }
    }

    pub fn get_current_acc(&self) -> u64 {
        self.acc.get()
    }

    fn notify_threshold(&self) {
        // Should we notify anyone?
        if let Some(threshold) = self.threshold.get() {
            let acc = self.acc.get();
            if acc >= threshold {
                // Yes, and we have enough guarantees.
                // Notify the waiting task, and reset the threshold. When the task is notified, it decreases the accumulated value again.
                self.notify.set(ThresholdOutcome::YayReached);
                self.threshold.set(None);
            } else {
                // Some task is waiting but we don't have enough guarantees. Check whether it is impossible to ever get enough.
                // If so, notify the task. Otherwise, do nothing
                if let Some(bound) = self.bound.get_bound() {
                    if threshold > acc.saturating_add(bound) {
                        self.notify.set(ThresholdOutcome::NopeBound);
                        self.threshold.set(None);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ThresholdOutcome::*;

    use smol::{block_on, Timer};

    #[test]
    fn test_immediately_available() {
        let guarantees = GuaranteeCell::new();

        block_on(async {
            futures::join!(
                async {
                    assert_eq!(Ok(()), guarantees.add_guarantees(9999));
                },
                async {
                    assert_eq!(YayReached, guarantees.await_threshold_sub(12).await);
                    assert_eq!(9999 - 12, guarantees.get_current_acc());
                    assert_eq!(YayReached, guarantees.await_threshold_sub(30).await);
                    assert_eq!(9999 - (12 + 30), guarantees.get_current_acc());
                }
            );
        });
    }

    #[test]
    fn test_becoming_available() {
        let guarantees = GuaranteeCell::new();

        block_on(async {
            futures::join!(
                async {
                    assert_eq!(YayReached, guarantees.await_threshold_sub(12).await);
                    assert_eq!(YayReached, guarantees.await_threshold_sub(30).await);
                    assert_eq!(YayReached, guarantees.await_threshold_sub(17).await);
                },
                async {
                    assert_eq!(Ok(()), guarantees.add_guarantees(44));
                    assert_eq!(Ok(()), guarantees.add_guarantees(43));
                },
            );
        });
    }

    #[test]
    fn test_immediately_impossible() {
        let guarantees = GuaranteeCell::new();

        block_on(async {
            futures::join!(
                async {
                    assert_eq!(Ok(()), guarantees.apply_bound(11));
                },
                async {
                    assert_eq!(NopeBound, guarantees.await_threshold_sub(12).await);
                }
            );
        });
    }

    #[test]
    fn test_becoming_impossible() {
        let guarantees = GuaranteeCell::new();

        block_on(async {
            futures::join!(
                async {
                    assert_eq!(NopeBound, guarantees.await_threshold_sub(12).await);
                },
                async {
                    assert_eq!(Ok(()), guarantees.add_guarantees(8));
                    assert_eq!(Ok(()), guarantees.apply_bound(500));
                    assert_eq!(Ok(()), guarantees.apply_bound(3));
                },
            );
        });
    }

    #[test]
    fn test_becoming_available_within_bound() {
        let guarantees = GuaranteeCell::new();

        block_on(async {
            futures::join!(
                async {
                    assert_eq!(Ok(()), guarantees.apply_bound(500));
                    assert_eq!(YayReached, guarantees.await_threshold_sub(12).await);
                    assert_eq!(YayReached, guarantees.await_threshold_sub(30).await);
                    assert_eq!(YayReached, guarantees.await_threshold_sub(17).await);
                },
                async {
                    assert_eq!(Ok(()), guarantees.add_guarantees(44));
                    assert_eq!(Ok(()), guarantees.add_guarantees(43));
                },
            );
        });
    }

    #[test]
    fn test_invalid_bound() {
        let guarantees = GuaranteeCell::new();
        assert_eq!(Ok(()), guarantees.apply_bound(500));
        assert_eq!(Err(500), guarantees.apply_bound(600));
    }

    #[test]
    fn test_disrespecting_bound() {
        let guarantees = GuaranteeCell::new();
        assert_eq!(Ok(()), guarantees.apply_bound(500));
        assert_eq!(Ok(()), guarantees.add_guarantees(400));
        assert_eq!(Err(()), guarantees.add_guarantees(101));
    }
}
