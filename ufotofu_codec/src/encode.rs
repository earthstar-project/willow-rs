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

/// Methods for encoding a value that belongs to an *encoding relation*.
///
/// API contracts:
///
/// - The encoding must not depend on details of the consumer such as when it yields or how many item slots it exposes at a time.
/// - Nonequal values must result in nonequal encodings.
/// - No encoding must be a prefix of a different encoding.
/// - For types that also implement [`Decodable`](crate::Decodable) and [`Eq`], encoding a value and then decoding it must yield a value equal to the original.
pub trait Encodable {
    /// Writes an encoding of `&self` into the given consumer.
    fn encode<C>(&self, consumer: &mut C) -> impl Future<Output = Result<(), C::Error>>
    where
        C: BulkConsumer<Item = u8>;

    #[cfg(feature = "alloc")]
    /// Encodes into a [Vec] instead of a given consumer.
    fn encode_into_vec(&self) -> impl Future<Output = Vec<u8>> {
        async {
            let mut c = IntoVec::new();

            match self.encode(&mut c).await {
                Ok(()) => c.into_vec(),
                Err(_) => unreachable!(),
            }
        }
    }
}

/// Encodables that can (efficiently and synchronously) precompute the length of their encoding.
///
/// API contract: `self.encode(c)` must write exactly `self.len_of_encoding()` many bytes into `c`.
pub trait EncodableKnownSize: Encodable {
    /// Computes the size of the encoding in bytes. Calling [`encode`](Encodable::encode) must feed exactly that many bytes into the consumer.
    fn len_of_encoding(&self) -> usize;

    #[cfg(feature = "alloc")]
    /// Encodes into a boxed slice instead of a given consumer.
    fn encode_into_boxed_slice(&self) -> impl Future<Output = Box<[u8]>> {
        async {
            let mut c = IntoVec::with_capacity(self.len_of_encoding());

            match self.encode(&mut c).await {
                Ok(()) => c.into_vec().into_boxed_slice(),
                Err(_) => unreachable!(),
            }
        }
    }
}

/// An encodable that introduces no asynchrony beyond that of `.await`ing the consumer. This is essentially a marker trait by which to tell other programmers about this property. As a practical benefit, the default methods of this trait allow for convenient synchronous encoding by internally using consumers that are known to never block.
pub trait EncodableSync: Encodable {
    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a Vec instead of a given consumer.
    fn sync_encode_into_vec(&self) -> Vec<u8> {
        pollster::block_on(self.encode_into_vec())
    }

    #[cfg(feature = "alloc")]
    /// Synchronously encodes into a boxed slice instead of a given consumer.
    fn sync_encode_into_boxed_slice(&self) -> Box<[u8]>
    where
        Self: EncodableKnownSize,
    {
        pollster::block_on(self.encode_into_boxed_slice())
    }
}
