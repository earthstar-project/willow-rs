#![no_std]

//! # UFOTOFU Codec Endian
//!
//! [TODO] write documentation.

use core::convert::Infallible;

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use arbitrary::Arbitrary;

use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::Decodable;
use ufotofu_codec::DecodableCanonic;
use ufotofu_codec::DecodableSync;
use ufotofu_codec::DecodeError;
use ufotofu_codec::Encodable;
use ufotofu_codec::EncodableKnownSize;
use ufotofu_codec::EncodableSync;

macro_rules! endian_wrapper {
    (name $wrapper_name:ident; integer $int:ident; doc_array $arr:expr; from_bytes $from_bytes:ident; to_bytes $to_bytes:ident; word $word:ident) => {
        #[doc = concat!("A thin wrapper around [`", stringify!($int), "`] for ", stringify!($word), "-endian encoding.
```
use ufotofu_codec_endian::*;
use ufotofu_codec::*;

assert_eq!(
    &", stringify!($arr), "[..],
    &", stringify!($wrapper_name), "(0x12cd).sync_encode_into_boxed_slice()[..]
);
assert_eq!(
    0x12cd,
    ", stringify!($wrapper_name), "::sync_decode_from_slice(&", stringify!($arr), ").unwrap().0
);
```")]
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
        #[cfg_attr(feature = "std", derive(Arbitrary))]
        pub struct $wrapper_name(pub $int);

        impl From<$int> for $wrapper_name {
            fn from(value: $int) -> Self {
                $wrapper_name(value)
            }
        }

        impl From<$wrapper_name> for $int {
            fn from(value: $wrapper_name) -> Self {
                value.0
            }
        }

        impl Encodable for $wrapper_name {
            async fn encode<C>(
                &self,
                consumer: &mut C,
            ) -> Result<(), C::Error>
            where
                C: BulkConsumer<Item = u8>,
            {
                let bytes = self.0.$to_bytes();
                consumer
                    .bulk_consume_full_slice(&bytes[..])
                    .await.map_err(|err| err.into_reason())?;
                Ok(())
            }
        }

        impl EncodableKnownSize for $wrapper_name {
            fn len_of_encoding(&self) -> usize {
                ($int::BITS / 8) as usize
            }
        }

        impl EncodableSync for $wrapper_name {}

        impl Decodable for $wrapper_name {
            type ErrorReason = Infallible;

            async fn decode<P>(
                producer: &mut P,
            ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
            where
                P: BulkProducer<Item = u8>,
            {
                let mut bytes = [0u8; ($int::BITS / 8) as usize];
                producer.bulk_overwrite_full_slice(&mut bytes[..]).await?;
                Ok($wrapper_name($int::$from_bytes(bytes)))
            }
        }

        impl DecodableCanonic for $wrapper_name {
            type ErrorCanonic = Self::ErrorReason;

            async fn decode_canonic<P>(
                producer: &mut P,
            ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
            where
                P: BulkProducer<Item = u8>,
            {
                Self::decode(producer).await
            }
        }

        impl DecodableSync for $wrapper_name {}
    }
}

macro_rules! endian_wrapper_single_byte {
    (name $wrapper_name:ident; integer $int:ident; word $word:ident) => {
        #[doc = concat!("A thin wrapper around [`", stringify!($int), "`] for ", stringify!($word), "-endian encoding.
```
use ufotofu_codec_endian::*;
use ufotofu_codec::*;

assert_eq!(
    &[0x12][..],
    &", stringify!($wrapper_name), "(0x12).sync_encode_into_boxed_slice()[..]
);
assert_eq!(
    0x12,
    ", stringify!($wrapper_name), "::sync_decode_from_slice(&[0x12]).unwrap().0
);
```")]
        pub struct $wrapper_name(pub $int);

        impl From<$int> for $wrapper_name {
            fn from(value: $int) -> Self {
                $wrapper_name(value)
            }
        }

        impl From<$wrapper_name> for $int {
            fn from(value: $wrapper_name) -> Self {
                value.0
            }
        }

        impl Encodable for $wrapper_name {
            async fn encode<C>(
                &self,
                consumer: &mut C,
            ) -> Result<(), C::Error>
            where
                C: BulkConsumer<Item = u8>,
            {
                let bytes = self.0.to_be_bytes();
                consumer
                    .bulk_consume_full_slice(&bytes[..])
                    .await
                    .map_err(|err| err.into_reason())
            }
        }

        impl EncodableKnownSize for $wrapper_name {
            fn len_of_encoding(&self) -> usize {
                ($int::BITS / 8) as usize
            }
        }

        impl EncodableSync for $wrapper_name {}

        impl Decodable for $wrapper_name {
            type ErrorReason = Infallible;

            async fn decode<P>(
                producer: &mut P,
            ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
            where
                P: BulkProducer<Item = u8>,
            {
                let mut bytes = [0u8; ($int::BITS / 8) as usize];
                producer
                    .bulk_overwrite_full_slice(&mut bytes[..])
                    .await?;
                Ok($wrapper_name($int::from_be_bytes(bytes)))
            }
        }

        impl DecodableCanonic for $wrapper_name {
            type ErrorCanonic = Self::ErrorReason;

            async fn decode_canonic<P>(
                producer: &mut P,
            ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
            where
                P: BulkProducer<Item = u8>,
            {
                Self::decode(producer).await
            }
        }

        impl DecodableSync for $wrapper_name {}
    }
}

macro_rules! wrapper_be {
    (name $wrapper_name:ident; integer $int:ident; doc_array $arr:expr) => {
        endian_wrapper!(name $wrapper_name; integer $int; doc_array $arr; from_bytes from_be_bytes; to_bytes to_be_bytes; word big);
    }
}

macro_rules! wrapper_le {
    (name $wrapper_name:ident; integer $int:ident; doc_array $arr:expr) => {
        endian_wrapper!(name $wrapper_name; integer $int; doc_array $arr; from_bytes from_le_bytes; to_bytes to_le_bytes; word little);
    }
}

endian_wrapper_single_byte!(name U8BE; integer u8; word big);
wrapper_be!(name U16BE; integer u16; doc_array [0x12, 0xcd]);
wrapper_be!(name U32BE; integer u32; doc_array [0, 0, 0x12, 0xcd]);
wrapper_be!(name U64BE; integer u64; doc_array [0, 0, 0, 0, 0, 0, 0x12, 0xcd]);

endian_wrapper_single_byte!(name I8BE; integer i8; word big);
wrapper_be!(name I16BE; integer i16; doc_array [0x12, 0xcd]);
wrapper_be!(name I32BE; integer i32; doc_array [0, 0, 0x12, 0xcd]);
wrapper_be!(name I64BE; integer i64; doc_array [0, 0, 0, 0, 0, 0, 0x12, 0xcd]);

endian_wrapper_single_byte!(name U8LE; integer u8; word little);
wrapper_le!(name U16LE; integer u16; doc_array [0xcd, 0x12]);
wrapper_le!(name U32LE; integer u32; doc_array [0xcd, 0x12, 0, 0]);
wrapper_le!(name U64LE; integer u64; doc_array [0xcd, 0x12, 0, 0, 0, 0, 0, 0]);

endian_wrapper_single_byte!(name I8LE; integer i8; word little);
wrapper_le!(name I16LE; integer i16; doc_array [0xcd, 0x12]);
wrapper_le!(name I32LE; integer i32; doc_array [0xcd, 0x12, 0, 0]);
wrapper_le!(name I64LE; integer i64; doc_array [0xcd, 0x12, 0, 0, 0, 0, 0, 0]);
