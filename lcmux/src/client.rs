//! This file takes the code in `client_logic` and wraps it with a nicer, ufotofu-based interface.

use std::ops::DerefMut;

use ufotofu::BulkConsumer;
use ufotofu_codec::{Encodable, EncodableKnownSize};
use ufotofu_codec_endian::{U16BE, U32BE, U64BE};
use wb_async_utils::Mutex;

pub use crate::client_logic::{AbsolutionsToGrant, Input};
use crate::client_logic::{new_logical_channel_client_logic_state, WaitForGuarantees};


/// A consumer that respects the guarantees available on a logical channel.
pub struct LogicalChannelClientEndpoint<'transport, C> {
    consumer: &'transport Mutex<C>,
    /// The lcmux channel id. TODO rename to channel_id
    channel: u64,
    guarantees: WaitForGuarantees,
}

/// The `Error` type for a consumer for logical channel. A final value is either one from the underlying `BulkConsumer` (of type `E`), or a dedicated variant to indicate that the logical channel was closed by the peer.
pub enum LogicalChannelClientError<E> {
    /// The underlying `BulkConsumer` errored.
    Underlying(E),
    /// The peer closed this logical channel, so it must reject future values.
    LogicalChannelClosed,
}

impl<E> From<E> for LogicalChannelClientError<E> {
    fn from(err: E) -> Self {
        Self::Underlying(err)
    }
}

impl<'transport, C> LogicalChannelClientEndpoint<'transport, C>
where
    C: BulkConsumer<Item = u8>,
{
    pub async fn send_to_channel<T: EncodableKnownSize>(
        &mut self,
        message: T,
    ) -> Result<(), LogicalChannelClientError<C::Error>> {
        let size = message.len_of_encoding() as u64;

        // Compute how many bytes to encode the length in.
        let floored_base256_log_of_size = (size.ilog2() / 8) as u8;
        // Shift to the 2nd to 2th most significant bits to form the first four bits of the header byte.
        let mut first_byte = floored_base256_log_of_size << 3;
        first_byte |= channel_id_to_four_header_bits(self.channel);

        if let Err(()) = self.guarantees.wait_for_guarantees(size).await {
            return Err(LogicalChannelClientError::LogicalChannelClosed);
        }

        // Acquire exclusive access to the consumer while encoding the message.
        let mut c = self.consumer.write().await;

        // Send the header byte.
        c.deref_mut().consume(first_byte).await?;
        // Send the encoding of the channel (might be zero bytes long).
        encode_channel_id(c.deref_mut(), self.channel).await?;
        // Send the encoding of the length of the `size` of the message as a `floored_base64_log_of_size + 1` byte integer.
        encode_size(c.deref_mut(), size).await?;

        // Done with the header, actually encode the message.
        message.encode(c.deref_mut()).await?;

        return Ok(());
    }
}

// Most significant four bits are zero, less significant four bits are set according to spec.
fn channel_id_to_four_header_bits(id: u64) -> u8 {
    // TODO use CompactU64 instead
    if id <= 11 {
        id as u8
    } else if id < 256 {
        12
    } else if id < 256 * 256 {
        13
    } else if id < 256 * 256 * 256 * 256 {
        14
    } else {
        15
    }
}

async fn encode_channel_id<C: BulkConsumer<Item = u8>>(
    consumer: &mut C,
    id: u64,
) -> Result<(), C::Error> {
    if id <= 11 {
        Ok(())
    } else if id < 256 {
        consumer.consume(id as u8).await
    } else if id < 256 * 256 {
        U16BE::from(id as u16).encode(consumer).await
    } else if id < 256 * 256 * 256 * 256 {
        U32BE::from(id as u32).encode(consumer).await
    } else {
        U64BE::from(id).encode(consumer).await
    }
}

async fn encode_size<C: BulkConsumer<Item = u8>>(
    consumer: &mut C,
    size: u64,
) -> Result<(), C::Error> {
    let num_bytes = ((size.ilog2() / 8) as u8) + 1;
    let as_bytes = size.to_be_bytes();

    consumer
        .bulk_consume_full_slice(&as_bytes[8usize - (num_bytes as usize)..])
        .await
        .map_err(|err| err.reason)
}

/// Creates the three parts of the client-side implementation of a logical channel.
pub fn new_logical_channel_client_state<'transport, C>(
    consumer: &'transport Mutex<C>,
    channel: u64,
) -> (
    Input,
    LogicalChannelClientEndpoint<'transport, C>,
    AbsolutionsToGrant,
) {
    let (input, wfg, atg) = new_logical_channel_client_logic_state();
    let lccc = LogicalChannelClientEndpoint {
        channel,
        guarantees: wfg,
        consumer,
    };

    return (input, lccc, atg);
}
