#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamespaceId([u8; 32]);

impl From<[u8; 32]> for NamespaceId {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl NamespaceId {
    /// Returns the raw bytes that make up this namespace id.
    ///
    /// This type deliberately does not provide implementations of [`AsRef`], [`Deref`](core::ops::Deref) or [`Borrow`](core::borrow::Borrow), to make it more difficult to leak information accidentally. Apologies if this is inconvenient.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubspaceId([u8; 32]);

impl From<[u8; 32]> for SubspaceId {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl SubspaceId {
    /// Returns the raw bytes that make up this subspace id.
    ///
    /// This type deliberately does not provide implementations of [`AsRef`], [`Deref`](core::ops::Deref) or [`Borrow`](core::borrow::Borrow), to make it more difficult to leak information accidentally. Apologies if this is inconvenient.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PayloadDigest([u8; 32]);

impl From<[u8; 32]> for PayloadDigest {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl PayloadDigest {
    /// Returns the raw bytes that make up this payload digest.
    ///
    /// This type deliberately does not provide implementations of [`AsRef`], [`Deref`](core::ops::Deref) or [`Borrow`](core::borrow::Borrow), to make it more difficult to leak information accidentally. Apologies if this is inconvenient.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
