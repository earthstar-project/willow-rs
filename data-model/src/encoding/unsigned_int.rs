use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::encoding::error::{DecodeError, EncodingConsumerError};
use crate::encoding::parameters::{Decoder, Encoder};

/// A big-endian encoded 8-bit unsigned integer.
pub struct U8BE(u8);

impl Encoder for U8BE {
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let byte = self.0.to_be_bytes();
        consumer.bulk_consume_full_slice(&byte).await?;
        Ok(())
    }
}

impl Decoder for U8BE {
    async fn decode<Producer>(producer: &mut Producer) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut bytes = [0u8; 1];
        producer.bulk_overwrite_full_slice(&mut bytes).await?;
        Ok(U8BE(u8::from_be_bytes(bytes)))
    }
}

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

/// A big-endian encoded 16-bit unsigned integer.
pub struct U16BE(u16);

impl Encoder for U16BE {
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let bytes = self.0.to_be_bytes();
        consumer.bulk_consume_full_slice(&bytes).await?;
        Ok(())
    }
}

impl Decoder for U16BE {
    async fn decode<Producer>(producer: &mut Producer) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut bytes = [0u8; 2];
        producer.bulk_overwrite_full_slice(&mut bytes).await?;
        Ok(U16BE(u16::from_be_bytes(bytes)))
    }
}

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

/// A big-endian encoded 32-bit unsigned integer.
pub struct U32BE(u32);

impl Encoder for U32BE {
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let bytes = self.0.to_be_bytes();
        consumer.bulk_consume_full_slice(&bytes).await?;
        Ok(())
    }
}

impl Decoder for U32BE {
    async fn decode<Producer>(producer: &mut Producer) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut bytes = [0u8; 4];
        producer.bulk_overwrite_full_slice(&mut bytes).await?;
        Ok(U32BE(u32::from_be_bytes(bytes)))
    }
}

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

/// A big-endian encoded 64-bit unsigned integer.
pub struct U64BE(u64);

impl Encoder for U64BE {
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let bytes = self.0.to_be_bytes();
        consumer.bulk_consume_full_slice(&bytes).await?;
        Ok(())
    }
}

impl Decoder for U64BE {
    async fn decode<Producer>(producer: &mut Producer) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut bytes = [0u8; 8];
        producer.bulk_overwrite_full_slice(&mut bytes).await?;
        Ok(U64BE(u64::from_be_bytes(bytes)))
    }
}

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
