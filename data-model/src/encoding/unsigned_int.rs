use crate::encoding::error::DecodeError;

use core::mem::size_of;

/// A `u8` wrapper that implements [`crate::encoding::Encodable`] and [`crate::encoding::Decodable`] by encoding as a big-endian fixed-width integer.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct U8BE(u8);

impl From<u8> for U8BE {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<U8BE> for u64 {
    fn from(value: U8BE) -> Self {
        value.0 as u64
    }
}

/// A `u16` wrapper that implements [`crate::encoding::Encodable`] and [`crate::encoding::Decodable`] by encoding as a big-endian fixed-width integer.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct U16BE(u16);

impl From<u16> for U16BE {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<U16BE> for u64 {
    fn from(value: U16BE) -> Self {
        value.0 as u64
    }
}

/// A `u32` wrapper that implements [`crate::encoding::Encodable`] and [`crate::encoding::Decodable`] by encoding as a big-endian fixed-width integer.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct U32BE(u32);

impl From<u32> for U32BE {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<U32BE> for u64 {
    fn from(value: U32BE) -> Self {
        value.0 as u64
    }
}

/// A `u64` wrapper that implements [`crate::encoding::Encodable`] and [`crate::encoding::Decodable`] by encoding as a big-endian fixed-width integer.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct U64BE(u64);

impl From<u64> for U64BE {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<U64BE> for u64 {
    fn from(value: U64BE) -> Self {
        value.0
    }
}

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    #[syncify_replace(use crate::encoding::sync::{Decodable, Encodable};)]
    use crate::encoding::{Decodable, Encodable};

    impl Encodable for U8BE {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let byte = self.0.to_be_bytes();
            consumer
                .bulk_consume_full_slice(&byte)
                .await
                .map_err(|f| f.reason)?;
            Ok(())
        }
    }

    impl Decodable for U8BE {
        async fn decode<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let mut bytes = [0u8; size_of::<u8>()];
            producer.bulk_overwrite_full_slice(&mut bytes).await?;
            Ok(U8BE(u8::from_be_bytes(bytes)))
        }
    }

    impl Encodable for U16BE {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let bytes = self.0.to_be_bytes();
            consumer
                .bulk_consume_full_slice(&bytes)
                .await
                .map_err(|f| f.reason)?;
            Ok(())
        }
    }

    impl Decodable for U16BE {
        async fn decode<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let mut bytes = [0u8; size_of::<u16>()];
            producer.bulk_overwrite_full_slice(&mut bytes).await?;
            Ok(U16BE(u16::from_be_bytes(bytes)))
        }
    }

    impl Encodable for U32BE {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let bytes = self.0.to_be_bytes();
            consumer
                .bulk_consume_full_slice(&bytes)
                .await
                .map_err(|f| f.reason)?;
            Ok(())
        }
    }

    impl Decodable for U32BE {
        async fn decode<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let mut bytes = [0u8; size_of::<u32>()];
            producer.bulk_overwrite_full_slice(&mut bytes).await?;
            Ok(U32BE(u32::from_be_bytes(bytes)))
        }
    }

    impl Encodable for U64BE {
        async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let bytes = self.0.to_be_bytes();
            consumer
                .bulk_consume_full_slice(&bytes)
                .await
                .map_err(|f| f.reason)?;
            Ok(())
        }
    }

    impl Decodable for U64BE {
        async fn decode<Producer>(
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let mut bytes = [0u8; size_of::<u64>()];
            producer.bulk_overwrite_full_slice(&mut bytes).await?;
            Ok(U64BE(u64::from_be_bytes(bytes)))
        }
    }
}
