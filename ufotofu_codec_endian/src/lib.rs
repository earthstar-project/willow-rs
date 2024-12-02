#![no_std]

//! # UFOTOFU Codec Endian
//!
//! [TODO] write documentation.

use core::fmt::Display;
use core::fmt::Formatter;

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

/// The possible error conditions why decoding a fixed-width integer might fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeFixedWidthError<ProducerError> {
    /// The producer of the bytes to be decoded emitted an error.
    Producer(ProducerError),
    /// The producer emitted its final item before decoding could be completed.
    UnexpectedEndOfInput,
}

impl<E> From<DecodeFixedWidthError<E>> for DecodeError<E> {
    fn from(value: DecodeFixedWidthError<E>) -> DecodeError<E> {
        match value {
            DecodeFixedWidthError::Producer(err) => DecodeError::Producer(err),
            DecodeFixedWidthError::UnexpectedEndOfInput => DecodeError::InvalidInput,
        }
    }
}

impl<E> From<E> for DecodeFixedWidthError<E> {
    fn from(err: E) -> Self {
        DecodeFixedWidthError::Producer(err)
    }
}

impl<E: Display> Display for DecodeFixedWidthError<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeFixedWidthError::Producer(err) => {
                write!(f, "The underlying producer encountered an error: {}", err,)
            }
            DecodeFixedWidthError::UnexpectedEndOfInput => {
                write!(f, "The input ended too soon.",)
            }
        }
    }
}

#[cfg(feature = "std")]
impl<E> Error for DecodeFixedWidthError<E>
where
    E: 'static + Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeFixedWidthError::Producer(err) => Some(err),
            DecodeFixedWidthError::UnexpectedEndOfInput => None,
        }
    }
}

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
            type EncodeRelativeTo = ();

            async fn relative_encode<C>(
                &self,
                consumer: &mut C,
                _r: &Self::EncodeRelativeTo,
            ) -> Result<(), C::Error>
            where
                C: BulkConsumer<Item = u8>,
            {
                let bytes = self.0.$to_bytes();
                consumer
                    .bulk_consume_full_slice(&bytes[..])
                    .await
                    .map_err(|err| err.into_reason())
            }
        }

        impl EncodableKnownSize for $wrapper_name {
            fn relative_len_of_encoding(&self, _r: &Self::EncodeRelativeTo) -> usize {
                ($int::BITS / 8) as usize
            }
        }

        impl EncodableSync for $wrapper_name {}

        impl Decodable for $wrapper_name {
            type Error<ProducerError> = DecodeFixedWidthError<ProducerError>;

            type DecodeRelativeTo = ();

            async fn relative_decode<P>(
                producer: &mut P,
                _r: &Self::DecodeRelativeTo,
            ) -> Result<Self, Self::Error<P::Error>>
            where
                P: BulkProducer<Item = u8>,
                Self: Sized,
            {
                let mut bytes = [0u8; ($int::BITS / 8) as usize];
                match producer
                    .bulk_overwrite_full_slice(&mut bytes[..])
                    .await
                    .map_err(|err| err.into_reason())
                {
                    Ok(()) => Ok($wrapper_name($int::$from_bytes(bytes))),
                    Err(Left(_fin)) => Err(DecodeFixedWidthError::UnexpectedEndOfInput),
                    Err(Right(err)) => Err(DecodeFixedWidthError::Producer(err)),
                }
            }
        }

        impl DecodableCanonic for $wrapper_name {
            type ErrorCanonic<ProducerError> = Self::Error<ProducerError>;

            async fn relative_decode_canonic<P>(
                producer: &mut P,
                r: &Self::DecodeRelativeTo,
            ) -> Result<Self, Self::ErrorCanonic<P::Error>>
            where
                P: BulkProducer<Item = u8>,
                Self: Sized,
            {
                Self::relative_decode(producer, r).await
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
            type EncodeRelativeTo = ();

            async fn relative_encode<C>(
                &self,
                consumer: &mut C,
                _r: &Self::EncodeRelativeTo,
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
            fn relative_len_of_encoding(&self, _r: &Self::EncodeRelativeTo) -> usize {
                ($int::BITS / 8) as usize
            }
        }

        impl EncodableSync for $wrapper_name {}

        impl Decodable for $wrapper_name {
            type Error<ProducerError> = DecodeFixedWidthError<ProducerError>;

            type DecodeRelativeTo = ();

            async fn relative_decode<P>(
                producer: &mut P,
                _r: &Self::DecodeRelativeTo,
            ) -> Result<Self, Self::Error<P::Error>>
            where
                P: BulkProducer<Item = u8>,
                Self: Sized,
            {
                let mut bytes = [0u8; ($int::BITS / 8) as usize];
                match producer
                    .bulk_overwrite_full_slice(&mut bytes[..])
                    .await
                    .map_err(|err| err.into_reason())
                {
                    Ok(()) => Ok($wrapper_name($int::from_be_bytes(bytes))),
                    Err(Left(_fin)) => Err(DecodeFixedWidthError::UnexpectedEndOfInput),
                    Err(Right(err)) => Err(DecodeFixedWidthError::Producer(err)),
                }
            }
        }

        impl DecodableCanonic for $wrapper_name {
            type ErrorCanonic<ProducerError> = Self::Error<ProducerError>;

            async fn relative_decode_canonic<P>(
                producer: &mut P,
                r: &Self::DecodeRelativeTo,
            ) -> Result<Self, Self::ErrorCanonic<P::Error>>
            where
                P: BulkProducer<Item = u8>,
                Self: Sized,
            {
                Self::relative_decode(producer, r).await
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
