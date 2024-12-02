#![no_std]

//! # Compact u64
//!
//! Compact encodings for unsigned 64-bit integers. The general idea is the following:
//!
//! - Each encoding is preceeded by a tag of two to eight (inclusive) bits.
//! - Each u64 can be encoded by setting the tag to the greatest possible number and then encoding the u64 as an eight-byte big-endian integer.
//! - Each u64 that fits into four bytes can be encoded by setting the tag to the second-greatest possible number and then encoding the u64 as an four-byte big-endian integer.
//! - Each u64 that fits into two bytes can be encoded by setting the tag to the third-greatest possible number and then encoding the u64 as an two-byte big-endian integer.
//! - Each u64 that fits into one byte can be encoded by setting the tag to the fourth-greatest possible number and then encoding the u64 as an one-byte big-endian integer.
//! - If the tag has more than two bits, then each u64 that is less than the fourth-greatest tag can be encoded in the tag directly, followed by no further bytes.
//!
//! The [`min_tag`] function computes the tag (stored in the least significant bits of the returned `u8`) for any given tag width and u64 that allows for the most compact encoding.
//!
//! The opaque [`EncodingWidth`] type represents the possible number of bytes that a u64 can be encoded in. [`EncodingWidth::min_width`] computes the width for the most compact encoding for any given tag width and u64.
//!
//! [`CompactU64`] is a thin wrapper around `u64` that implements the `ufotofu_codec` traits, encoding and decoding relative to arbitrary [`EncodingWidth`]s.

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::error::Error;

use either::Either::*;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::RelativeDecodable;
use ufotofu_codec::DecodableCanonic;
use ufotofu_codec::RelativeDecodableSync;
use ufotofu_codec::DecodeError;
use ufotofu_codec::Encodable;
use ufotofu_codec::EncodableKnownSize;
use ufotofu_codec::EncodableSync;

/// How many tag bits are available to indicate the width of a compact u64 encoding. Must be between two and eight inclusive.
pub type TagWidth = usize;

/// Returns the tag of a given tag-width for encoding a [`u64`] in the least number of bytes. Small tags are stored in the least significant bits of the return value.
///
/// ```
/// use compact_u64::*;
///
/// assert_eq!(11, min_tag(11, 4));
/// assert_eq!(12, min_tag(12, 4));
/// assert_eq!(12, min_tag(255, 4));
/// assert_eq!(13, min_tag(256, 4));
/// assert_eq!(13, min_tag(65535, 4));
/// assert_eq!(14, min_tag(65536, 4));
/// assert_eq!(14, min_tag(4294967295, 4));
/// assert_eq!(15, min_tag(4294967296, 4));
/// assert_eq!(15, min_tag(18446744073709551615, 4));
/// ```
pub const fn min_tag(n: u64, tag_width: TagWidth) -> u8 {
    debug_assert!(tag_width >= 2 && tag_width <= 8);
    let max_inline = (1 << tag_width) - 4;

    if n < max_inline {
        n as u8
    } else {
        let max_tag = (1 << tag_width) - 1;

        if n < 256 {
            max_tag - 3
        } else if n < 256 * 256 {
            max_tag - 2
        } else if n < 256 * 256 * 256 * 256 {
            max_tag - 1
        } else {
            max_tag
        }
    }
}

/// An opaque reprensentation of the possible width of a compact u64 encoding: zero, one, two, four, or eight bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EncodingWidth(u8);

impl EncodingWidth {
    /// Contructs an [`EncodingWidth`] from a given [`u8`], returing `None` if the argument is none of `0`, `1`, `2`, `4`, or `8`.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::from_u8(4).unwrap();
    /// assert_eq!(4, width.as_u8());
    ///
    /// assert_eq!(None, EncodingWidth::from_u8(5));
    /// ```
    pub const fn from_u8(num: u8) -> Option<Self> {
        match num {
            0 | 1 | 2 | 4 | 8 => Some(Self(num)),
            _ => None,
        }
    }

    /// Returns an [`EncodingWidth`] representing zero.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::zero();
    /// assert_eq!(0, width.as_u8());
    /// ```
    pub const fn zero() -> Self {
        EncodingWidth(0)
    }

    /// Returns an [`EncodingWidth`] representing one.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::one();
    /// assert_eq!(1, width.as_u8());
    /// ```
    pub const fn one() -> Self {
        EncodingWidth(1)
    }

    /// Returns an [`EncodingWidth`] representing two.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::two();
    /// assert_eq!(2, width.as_u8());
    /// ```
    pub const fn two() -> Self {
        EncodingWidth(2)
    }

    /// Returns an [`EncodingWidth`] representing four.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::four();
    /// assert_eq!(4, width.as_u8());
    /// ```
    pub const fn four() -> Self {
        EncodingWidth(4)
    }

    /// Returns an [`EncodingWidth`] representing eight.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::eight();
    /// assert_eq!(8, width.as_u8());
    /// ```
    pub const fn eight() -> Self {
        EncodingWidth(8)
    }

    /// Retrieves the width as a [`u8`].
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::one();
    /// assert_eq!(1, width.as_u8());
    /// ```
    pub const fn as_u8(&self) -> u8 {
        self.0
    }

    /// Retrieves the width as a [`usize`].
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = EncodingWidth::one();
    /// assert_eq!(1, width.as_usize());
    /// ```
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Returns the least [`EncodingWidth`] a given [`u64`] can be represented in, given a tag of `tag_width` bits.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// assert_eq!(0u8, EncodingWidth::min_width(11, 4).into());
    /// assert_eq!(1u8, EncodingWidth::min_width(12, 4).into());
    /// assert_eq!(1u8, EncodingWidth::min_width(255, 4).into());
    /// assert_eq!(2u8, EncodingWidth::min_width(256, 4).into());
    /// assert_eq!(2u8, EncodingWidth::min_width(65535, 4).into());
    /// assert_eq!(4u8, EncodingWidth::min_width(65536, 4).into());
    /// assert_eq!(4u8, EncodingWidth::min_width(4294967295, 4).into());
    /// assert_eq!(8u8, EncodingWidth::min_width(4294967296, 4).into());
    /// assert_eq!(8u8, EncodingWidth::min_width(18446744073709551615, 4).into());
    /// ```
    pub const fn min_width(n: u64, tag_width: TagWidth) -> EncodingWidth {
        debug_assert!(tag_width >= 2 && tag_width <= 8);
        let max_inline = (1 << tag_width) - 4;

        if n < max_inline {
            Self::zero()
        } else {
            if n < 256 {
                Self::one()
            } else if n < 256 * 256 {
                Self::two()
            } else if n < 256 * 256 * 256 * 256 {
                Self::four()
            } else {
                Self::eight()
            }
        }
    }

    /// Returns the tag of a given tag-width that indicates how to encode a given u64 in a given [`EncodingWidth`].
    ///
    /// Does not check whether the [`EncodingWidth`] is legal for the given u64 at the given tag_width.
    pub const fn corresponding_tag(&self, n: u64, tag_width: TagWidth) -> u8 {
        todo!()
    }
}

impl From<EncodingWidth> for u8 {
    fn from(value: EncodingWidth) -> Self {
        value.0
    }
}

impl From<EncodingWidth> for usize {
    fn from(value: EncodingWidth) -> Self {
        value.0 as usize
    }
}

/// A thin wrapper around `u64` that allows for encoding and decoding relative to arbitrary [`EncodingWidth`]s.
///
/// The implementation of [`DecodableCanonic`] first decodes a value and then checks whether a lesser tag could have also been used to encode the decoded value. If so, it reports an error.
///
/// ```
/// use compact_u64::*;
/// use ufotofu_codec::*;
///
/// assert_eq!(
///     &[123],
///     &CompactU64(123).sync_relative_encode_into_boxed_slice(&EncodingWidth::one())[..],
/// );
/// assert_eq!(
///     &[0, 123],
///     &CompactU64(123).sync_relative_encode_into_boxed_slice(&EncodingWidth::two())[..],
/// );
///
/// assert_eq!(
///     123,
///     CompactU64::sync_relative_decode_from_slice(&EncodingWidth::one(), &[123]).unwrap().0,
/// );
/// assert_eq!(
///     123,
///     CompactU64::sync_relative_decode_from_slice(&EncodingWidth::two(), &[0, 123]).unwrap().0,
/// );
///
/// assert!(
///     CompactU64::sync_relative_decode_canonic_from_slice(&EncodingWidth::two(), &[0, 123]).is_err(),
/// );
/// ```
pub struct CompactU64(pub u64);

impl From<u64> for CompactU64 {
    fn from(value: u64) -> Self {
        CompactU64(value)
    }
}

impl From<CompactU64> for u64 {
    fn from(value: CompactU64) -> Self {
        value.0
    }
}

impl Encodable for CompactU64 {
    type EncodeRelativeTo = EncodingWidth;

    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        r: &Self::EncodeRelativeTo,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        todo!()
    }
}

impl EncodableKnownSize for CompactU64 {
    fn relative_len_of_encoding(&self, r: &Self::EncodeRelativeTo) -> usize {
        todo!()
    }
}

impl EncodableSync for CompactU64 {}

impl RelativeDecodable for CompactU64 {
    type Error<ProducerError> = i16;

    type DecodeRelativeTo = i16;

    async fn relative_decode<P>(
        producer: &mut P,
        r: &Self::DecodeRelativeTo,
    ) -> Result<Self, Self::Error<P::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

impl DecodableCanonic for CompactU64 {
    type ErrorCanonic<ProducerError> = i16;

    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Self::DecodeRelativeTo,
    ) -> Result<Self, Self::ErrorCanonic<P::Error>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        todo!()
    }
}

impl RelativeDecodableSync for CompactU64 {}
