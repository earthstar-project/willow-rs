#![no_std]
#![allow(clippy::should_implement_trait)]

//! A hybrid between an [`UnsafeCell`](core::cell::UnsafeCell) and a [`RefCell`](core::cell::RefCell): comes with a [`RefCell`](core::cell::RefCell)-like but unsafe API that panics in test builds (`#[cfg(test)]`) when mutable access is not exclusive, but has no overhead (and allows for UB) in non-test builds.

#[cfg(test)]
mod checked;
#[cfg(test)]
pub use checked::*;

#[cfg(not(test))]
mod unchecked;
#[cfg(not(test))]
pub use unchecked::*;
