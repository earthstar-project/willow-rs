pub mod rw;
pub use rw::RwLock;

pub mod mutex;
pub use mutex::Mutex;

mod once_cell;
pub use once_cell::OnceCell;

mod take_cell;
pub use take_cell::TakeCell;

#[cfg(feature = "ufotofu_utils")]
pub mod spsc;

#[cfg(feature = "ufotofu_utils")]
pub mod shared_producer;

#[cfg(feature = "ufotofu_utils")]
pub mod shared_consumer;

#[cfg(feature = "ufotofu_utils")]
pub mod shelf;

// This is safe if and only if the object pointed at by `reference` lives for at least `'longer`.
// See https://doc.rust-lang.org/nightly/std/intrinsics/fn.transmute.html for more detail.
pub(crate) unsafe fn extend_lifetime<'shorter, 'longer, T: ?Sized>(
    reference: &'shorter T,
) -> &'longer T {
    core::mem::transmute::<&'shorter T, &'longer T>(reference)
}

// This is safe if and only if the object pointed at by `reference` lives for at least `'longer`.
// See https://doc.rust-lang.org/nightly/std/intrinsics/fn.transmute.html for more detail.
pub(crate) unsafe fn extend_lifetime_mut<'shorter, 'longer, T: ?Sized>(
    reference: &'shorter mut T,
) -> &'longer mut T {
    core::mem::transmute::<&'shorter mut T, &'longer mut T>(reference)
}
