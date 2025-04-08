#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use core::future::Future;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
#[cfg(all(feature = "std", not(feature = "alloc")))]
use std::{boxed::Box, collections::Vec};

use ufotofu::{consumer::IntoVec, BulkConsumer};

use crate::*;

/// Like [`Encodable`], but relative to some known value of type `RelativeTo`.
pub trait RelativeEncodable<RelativeTo> {
    /// Writes an encoding of `&self` into the given consumer.
    fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &RelativeTo,
    ) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>;

    #[cfg(feature = "alloc")]
    /// Encodes into a Vec instead of a given consumer.
    fn relative_encode_into_vec(&self, r: &RelativeTo) -> impl Future<Output = Vec<u8>> {
        async {
            let mut c = IntoVec::new();

            match self.relative_encode(&mut c, r).await {
                Ok(()) => c.into_vec(),
                Err(_) => unreachable!(),
            }
        }
    }
}

impl<T> RelativeEncodable<()> for T
where
    T: Encodable,
{
    fn relative_encode<C>(
        &self,
        consumer: &mut C,
        _r: &(),
    ) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>,
    {
        self.encode(consumer)
    }
}

/// Like [`EncodableKnownSize`], but relative to some known value of type `RelativeTo`.
pub trait RelativeEncodableKnownSize<RelativeTo>: RelativeEncodable<RelativeTo> {
    /// Computes the size of the encoding in bytes. Calling [`encode`](Encodable::encode) must feed exactly that many bytes into the consumer.
    fn relative_len_of_encoding(&self, r: &RelativeTo) -> usize;

    #[cfg(feature = "alloc")]
    /// Encodes into a boxed slice instead of a given consumer.
    fn relative_encode_into_boxed_slice(&self, r: &RelativeTo) -> impl Future<Output = Box<[u8]>> {
        async {
            let mut c = IntoVec::with_capacity(self.relative_len_of_encoding(r));

            match self.relative_encode(&mut c, r).await {
                Ok(()) => c.into_vec().into_boxed_slice(),
                Err(_) => unreachable!(),
            }
        }
    }
}

impl<T> RelativeEncodableKnownSize<()> for T
where
    T: EncodableKnownSize,
{
    fn relative_len_of_encoding(&self, _r: &()) -> usize {
        self.len_of_encoding()
    }
}

/// Like [`EncodableSync`], but relative to some known value of type `RelativeTo`.
pub trait RelativeEncodableSync<RelativeTo>: RelativeEncodable<RelativeTo> {
    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a Vec instead of a given consumer.
    fn sync_relative_encode_into_vec(&self, r: &RelativeTo) -> Vec<u8> {
        pollster::block_on(self.relative_encode_into_vec(r))
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a boxed slice instead of a given consumer.
    fn sync_relative_encode_into_boxed_slice(&self, r: &RelativeTo) -> Box<[u8]>
    where
        Self: RelativeEncodableKnownSize<RelativeTo>,
    {
        pollster::block_on(self.relative_encode_into_boxed_slice(r))
    }
}

impl<T> RelativeEncodableSync<()> for T where T: EncodableSync {}
