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
//! [`TagWidth`] is the type of possible tag widths (integers between two and eight inclusive). [`EncodingWidth`] is the type of the possible numbers of bytes that are needed for encoding a compact u64 beyond its tag: zero, one, two, four, or eight. [`EncodingWidth::min_width`] takes a `u64` and a [`TagWidth`], and returns the minimal [`EncodingWidth`] for compactly encoding the given number with a tag of the given width.
//!
//! The [`Tag`] type represents a tag: its width, together with the actual tag data. Its [`Tag::from_raw`] method can be used to create [`Tag`]s from single bytes when decoding compact u64s.
//!
//! Finally, the [`CompactU64`] type is the central type of the crate, a wrapper around `u64` that allows for encoding and decoding. It implements [`Encodable`] for emitting a minimal eight-bit tag followed by the minimal number of bytes for encoding the u64. The [`Decodable`] implementation conversely reads a single byte as an eight-bit tag, and then reads further bytes according to the tag to obtain the u64. There is also a [`DecodableCanonic`] implementation wihch errors if the decoded bytes are not the shortest possible encoding of the u64.
//!
//! Further, [`CompactU64`] implements [`RelativeEncodable`] for encoding only the number (but *not* the tag) relative to any given [`EncodingWidth`], and [`RelativeDecodable`] for decoding the number (but *not* a tag) relative to any given [`Tag`]. The corresponding [`RelativeDecodableCanonic`] implementation again enforces minimal encodings.

#[cfg(feature = "std")]
extern crate std;

use core::convert::Infallible;
use core::fmt::Display;

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::Blame;
use ufotofu_codec::Decodable;
use ufotofu_codec::DecodableCanonic;
use ufotofu_codec::DecodableSync;
use ufotofu_codec::DecodeError;
use ufotofu_codec::Encodable;
use ufotofu_codec::EncodableKnownSize;
use ufotofu_codec::EncodableSync;
use ufotofu_codec::RelativeDecodable;
use ufotofu_codec::RelativeDecodableCanonic;
use ufotofu_codec::RelativeDecodableSync;
use ufotofu_codec::RelativeEncodable;
use ufotofu_codec::RelativeEncodableKnownSize;
use ufotofu_codec::RelativeEncodableSync;
use ufotofu_codec_endian::{U16BE, U32BE, U64BE, U8BE};

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

/// An opaque representation of one of the possible tag widths: 2, 3, 4, 5, 6, 7, or 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TagWidth(u8);

impl TagWidth {
    /// Contructs an [`TagWidth`] from a given [`u8`], returing `None` if the argument is not between two and eight inclusive.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::from_u8(4).unwrap();
    /// assert_eq!(4, width.as_u8());
    ///
    /// assert_eq!(None, TagWidth::from_u8(9));
    /// ```
    pub const fn from_u8(num: u8) -> Option<Self> {
        match num {
            2..=8 => Some(Self(num)),
            _ => None,
        }
    }

    /// Returns a [`TagWidth`] representing two.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::two();
    /// assert_eq!(2, width.as_u8());
    /// ```
    pub const fn two() -> Self {
        TagWidth(2)
    }

    /// Returns a [`TagWidth`] representing three.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::three();
    /// assert_eq!(3, width.as_u8());
    /// ```
    pub const fn three() -> Self {
        TagWidth(3)
    }

    /// Returns a [`TagWidth`] representing four.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::four();
    /// assert_eq!(4, width.as_u8());
    /// ```
    pub const fn four() -> Self {
        TagWidth(4)
    }

    /// Returns a [`TagWidth`] representing five.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::five();
    /// assert_eq!(5, width.as_u8());
    /// ```
    pub const fn five() -> Self {
        TagWidth(5)
    }

    /// Returns a [`TagWidth`] representing six.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::six();
    /// assert_eq!(6, width.as_u8());
    /// ```
    pub const fn six() -> Self {
        TagWidth(6)
    }

    /// Returns a [`TagWidth`] representing seven.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::seven();
    /// assert_eq!(7, width.as_u8());
    /// ```
    pub const fn seven() -> Self {
        TagWidth(7)
    }

    /// Returns a [`TagWidth`] representing eight.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::eight();
    /// assert_eq!(8, width.as_u8());
    /// ```
    pub const fn eight() -> Self {
        TagWidth(8)
    }

    /// Retrieves the tag width as a [`u8`].
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::three();
    /// assert_eq!(3, width.as_u8());
    /// ```
    pub const fn as_u8(&self) -> u8 {
        self.0
    }

    /// Retrieves the tag width as a [`usize`].
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let width = TagWidth::three();
    /// assert_eq!(3, width.as_usize());
    /// ```
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Returns the maximal tag of the given width as a u8, i.e., a bunch of one bits in the least significant positions.
    const fn maximal_tag(&self) -> u8 {
        ((1_u16 << self.as_u8()) as u8).wrapping_sub(1)
    }
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for TagWidth {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> Result<Self, arbitrary::Error> {
        let raw = u8::arbitrary(u)?;
        match TagWidth::from_u8(raw % 8) {
            None => Ok(TagWidth::eight()),
            Some(tw) => Ok(tw),
        }
    }
}

/// An opaque representation of the possible width of a compact u64 encoding (excluding the tag): zero, one, two, four, or eight bytes.
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
    /// assert_eq!(0u8, EncodingWidth::min_width(11, TagWidth::four()).into());
    /// assert_eq!(1u8, EncodingWidth::min_width(12, TagWidth::four()).into());
    /// assert_eq!(1u8, EncodingWidth::min_width(255, TagWidth::four()).into());
    /// assert_eq!(2u8, EncodingWidth::min_width(256, TagWidth::four()).into());
    /// assert_eq!(2u8, EncodingWidth::min_width(65535, TagWidth::four()).into());
    /// assert_eq!(4u8, EncodingWidth::min_width(65536, TagWidth::four()).into());
    /// assert_eq!(4u8, EncodingWidth::min_width(4294967295, TagWidth::four()).into());
    /// assert_eq!(8u8, EncodingWidth::min_width(4294967296, TagWidth::four()).into());
    /// assert_eq!(8u8, EncodingWidth::min_width(18446744073709551615, TagWidth::four()).into());
    /// ```
    pub const fn min_width(n: u64, tag_width: TagWidth) -> EncodingWidth {
        let tag_width = tag_width.as_u8();
        let max_inline = (1_u64 << tag_width) - 4;

        if n < max_inline {
            Self::zero()
        } else if n < 256 {
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

/// A tag used for compact encoding. Combines a `Tagwidth` with a `u8` that stores the tag (in its least significant bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tag {
    width: TagWidth,
    data: u8,
}

impl Tag {
    /// Returns the tag of a given tag-width for encoding a [`u64`] in the least number of bytes.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// assert_eq!(11, Tag::min_tag(11, TagWidth::four()).data());
    /// assert_eq!(12, Tag::min_tag(12, TagWidth::four()).data());
    /// assert_eq!(12, Tag::min_tag(255, TagWidth::four()).data());
    /// assert_eq!(13, Tag::min_tag(256, TagWidth::four()).data());
    /// assert_eq!(13, Tag::min_tag(65535, TagWidth::four()).data());
    /// assert_eq!(14, Tag::min_tag(65536, TagWidth::four()).data());
    /// assert_eq!(14, Tag::min_tag(4294967295, TagWidth::four()).data());
    /// assert_eq!(15, Tag::min_tag(4294967296, TagWidth::four()).data());
    /// assert_eq!(15, Tag::min_tag(18446744073709551615, TagWidth::four()).data());
    /// ```
    pub const fn min_tag(n: u64, tag_width: TagWidth) -> Tag {
        let max_inline: u64 = (1_u64 << tag_width.as_u8()) - 4;

        let data = if n < max_inline {
            n as u8
        } else {
            let max_tag = tag_width.maximal_tag();

            if n < 256 {
                max_tag - 3
            } else if n < 256 * 256 {
                max_tag - 2
            } else if n < 256 * 256 * 256 * 256 {
                max_tag - 1
            } else {
                max_tag
            }
        };

        Tag {
            width: tag_width,
            data,
        }
    }

    /// Creates a [`Tag`] from a `u8` of data, a [`TagWidth`], and an offset where in the `u8` the tag begins. An offset of `0` indicates the most significant bit, an offset of `7` indicates the least significant bit. If the sum of width and offset is greater than eight, the function panics.
    ///
    /// ```
    /// use compact_u64::*;
    ///
    /// let tag = Tag::from_raw(0b0011_0100, TagWidth::four(), 2);
    /// assert_eq!(0b0000_1101, tag.data());
    /// assert_eq!(TagWidth::four(), tag.tag_width());
    /// assert_eq!(EncodingWidth::two(), tag.encoding_width());
    /// ```
    pub fn from_raw(raw: u8, width: TagWidth, offset: usize) -> Tag {
        match 8_usize.checked_sub(offset + width.as_usize()) {
            None => panic!("Invalid tag offset: {}", offset),
            Some(shift_by) => {
                let max_tag: u8 = width.maximal_tag();

                Tag {
                    width,
                    data: (raw >> shift_by) & max_tag,
                }
            }
        }
    }

    /// Returns the width of this [`Tag`].
    pub fn tag_width(&self) -> TagWidth {
        self.width
    }

    /// Returns the data of this [`Tag`], stored in the `self.width()` many least significant bits.
    pub fn data(&self) -> u8 {
        self.data
    }

    /// Returns the data of this [`Tag`], stored `self.width()` many bits starting at the given offset. An offset of zero denotes the most significant bit, an offset of seven the least significant bit.
    ///
    /// Panics if the sum of `offset` and the width of the tag is strictly greater than eight.
    pub fn data_at_offset(&self, offset: u8) -> u8 {
        debug_assert!(offset.saturating_add(self.tag_width().as_u8()) <= 8);
        self.data() << (8 - (self.tag_width().as_u8() + offset))
    }

    /// Returns the width of the integer encoding indicated by this tag.
    pub fn encoding_width(&self) -> EncodingWidth {
        match self.tag_width().maximal_tag() - self.data {
            0 => EncodingWidth::eight(),
            1 => EncodingWidth::four(),
            2 => EncodingWidth::two(),
            3 => EncodingWidth::one(),
            _ => EncodingWidth::zero(),
        }
    }
}

#[cfg(feature = "dev")]
impl<'a> Arbitrary<'a> for Tag {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> Result<Self, arbitrary::Error> {
        let width = TagWidth::arbitrary(u)?;
        let mask = width.maximal_tag();
        let raw = u8::arbitrary(u)?;
        let data = raw & mask;

        Ok(Tag { width, data })
    }
}

/// A thin wrapper around `u64` that allows for minimal encoding relative to arbitrary [`EncodingWidth`]s, and for decoding relative to arbitrary [`Tag`]s.
///
/// The implementation of [`DecodableCanonic`] first decodes a value and then checks whether a lesser tag could have also been used to encode the decoded value. If so, it reports an error.
///
/// Also implements absolute encoding and decoding by prefixing the number with its eight-bit tag.
///
/// ```
/// use compact_u64::*;
/// use ufotofu_codec::*;
///
/// assert_eq!(
///     &[123],
///     &CompactU64(123).sync_relative_encode_into_boxed_slice(&EncodingWidth::one())[..],
/// );
///
/// assert_eq!(
///     123,
///     CompactU64::sync_relative_decode_from_slice(&[], &Tag::from_raw(123, TagWidth::eight(), 0)).unwrap().0,
/// );
///
/// assert_eq!(
///     123,
///     CompactU64::sync_relative_decode_from_slice(&[123], &Tag::from_raw(252, TagWidth::eight(), 0)).unwrap().0,
/// );
/// assert_eq!(
///     123,
///     CompactU64::sync_relative_decode_from_slice(&[0, 123], &Tag::from_raw(253, TagWidth::eight(), 0)).unwrap().0,
/// );
///
/// assert!(
///     CompactU64::sync_relative_decode_canonic_from_slice(&[0, 123], &Tag::from_raw(253, TagWidth::eight(), 0)).is_err(),
/// );
///
/// assert_eq!(
///     &[123],
///     &CompactU64(123).sync_encode_into_boxed_slice()[..],
/// );
///
/// assert_eq!(
///     123,
///     CompactU64::sync_decode_from_slice(&[123]).unwrap().0,
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct CompactU64(
    /// The wrapped `u64`.
    pub u64,
);

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

/// Produces the shortest possible encodings relative to a given `EncodingWidth`. **Careful: Does not encode any tag itself. Use [`Tag::min_tag`] to obtain the corresponding tag.**
impl RelativeEncodable<EncodingWidth> for CompactU64 {
    async fn relative_encode<C>(&self, consumer: &mut C, r: &EncodingWidth) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        match r.as_u8() {
            0 => Ok(()),
            1 => U8BE(self.0 as u8).encode(consumer).await,
            2 => U16BE(self.0 as u16).encode(consumer).await,
            4 => U32BE(self.0 as u32).encode(consumer).await,
            8 => U64BE(self.0).encode(consumer).await,
            _ => unreachable!(),
        }
    }
}

impl RelativeEncodableKnownSize<EncodingWidth> for CompactU64 {
    fn relative_len_of_encoding(&self, r: &EncodingWidth) -> usize {
        r.as_usize()
    }
}

impl RelativeEncodableSync<EncodingWidth> for CompactU64 {}

impl RelativeDecodable<Tag, Infallible> for CompactU64 {
    async fn relative_decode<P>(
        producer: &mut P,
        r: &Tag,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Infallible>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        match r.encoding_width().as_u8() {
            0 => Ok(CompactU64(r.data() as u64)),
            1 => Ok(CompactU64(U8BE::decode(producer).await?.0 as u64)),
            2 => Ok(CompactU64(U16BE::decode(producer).await?.0 as u64)),
            4 => Ok(CompactU64(U32BE::decode(producer).await?.0 as u64)),
            8 => Ok(CompactU64(U64BE::decode(producer).await?.0)),
            _ => unreachable!(),
        }
    }
}

/// Writes an eight-bit tag before the big-endian number.
impl Encodable for CompactU64 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        let tag = Tag::min_tag(self.0, TagWidth::eight());
        consumer.consume(tag.data()).await?;
        self.relative_encode(consumer, &tag.encoding_width()).await
    }
}

impl EncodableKnownSize for CompactU64 {
    fn len_of_encoding(&self) -> usize {
        let tag = Tag::min_tag(self.0, TagWidth::eight());
        1 + self.relative_len_of_encoding(&tag.encoding_width())
    }
}

impl EncodableSync for CompactU64 {}

/// Expects an eight-bit tag before the big-endian number.
impl Decodable for CompactU64 {
    type ErrorReason = Infallible;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let tag_byte = producer.produce_item().await?;
        let tag = Tag::from_raw(tag_byte, TagWidth::eight(), 0);
        CompactU64::relative_decode(producer, &tag).await
    }
}

impl DecodableCanonic for CompactU64 {
    type ErrorCanonic = NotMinimal;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let tag_byte = producer.produce_item().await?;
        let tag = Tag::from_raw(tag_byte, TagWidth::eight(), 0);
        CompactU64::relative_decode_canonic(producer, &tag).await
    }
}

impl DecodableSync for CompactU64 {}

/// Marker unit struct to indicate that a compact u64 encoding was not minimal, as would be required for canonic decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotMinimal;

impl Display for NotMinimal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "A compact u64 decoding was not minimal.")
    }
}

impl From<NotMinimal> for Blame {
    fn from(_value: NotMinimal) -> Self {
        Blame::TheirFault
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NotMinimal {}

impl From<Infallible> for NotMinimal {
    fn from(_value: Infallible) -> Self {
        unreachable!()
    }
}

impl RelativeDecodableCanonic<Tag, Infallible, NotMinimal> for CompactU64 {
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &Tag,
    ) -> Result<Self, DecodeError<P::Final, P::Error, NotMinimal>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let decoded = Self::relative_decode(producer, r)
            .await
            .map_err(DecodeError::map_other_from)?;

        if r == &Tag::min_tag(decoded.0, r.tag_width()) {
            Ok(decoded)
        } else {
            Err(DecodeError::Other(NotMinimal))
        }
    }
}

impl RelativeDecodableSync<Tag, Infallible> for CompactU64 {}
