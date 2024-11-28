//! Components for the client side of logical channels, i.e., the side that receives guarantees and respects them. This implementation always respects the granted guarantees, it does not allow for optimistic sending, and has no way of handling the communication that can be caused by overly optimistic sending.
//!
//! For each logical channel to write to, the client tracks the available guarantees at any point in time. It further tracks incoming `Plead` messages. This implementation fully honours all `Plead` messages. Finally, it tracks the bounds communicated by incoming `ControlLimitReceiving` messages.
//!
//! On the outgoing side, the module allows for sending to the logical channel while respecting the available guarantees; the implementation does not support optimistic sending. It provides notifications when `Absolve` messages should be sent in response to incoming `Plead` messages; it does not support unsolicited absolutions. It sends a `LimitSending` message with a `bound` of zero when the `Consumer` representation of the logical channel is closed; it does not support earlier `LimitSending` messages with nonzero bounds.
//!
//! The implementation does not concern itself with incoming `AnnounceDropping` messages, nor with sending `Apologise` messages, because neither occurs (in communication with a correct peer) since it never sends optimistically.
//!
//! Note that this module does not deal with any sort of message encoding or decoding, it merely provides the machinery for tracking and modifying guarantees. The one exception is the sending of the `LimitSending` message when the consumer is closed, that one is encoded as bytes and sent directly into the underlying consumer.

// Implementing correct client behaviour requires working both with incoming data and sending data to an outgoing channel. Both happen concurrently, and both need to be able to modify the state that the client must track. Since rust is not keen on shared mutable state, we follow the common pattern of having a shared state inside some cell(s) inside an Rc, with multiple values having access to the Rc. In particular, we use `AsyncCell`s.

// We have three access points to the mutable state:
//
// - A receiving end that synchronously updates the state based on incoming `IssueGuarantee`, `Plead`, and `ControlLimitReceiving` messages (there are no facilities for processing `Apologise` messages, since this implementation never causes them): `Input`.
// - An end for sending messages on the logical channel: the program can asynchronously wait for a notification that a certain amount of guarantees has become available; this credit is considered to be used up once the async notification has been delivered: `WaitForGuarantees`.
// - An end for asynchronously listening for received `Plead` messages. This component generates information about what sort of `Absolve` messages the client must then send to keep everything consistent. Internally, every `Plead` message is immediately respected (i.e., the `WaitForGuarantees` endpoint will have to wait even longer): `AbsolutionsToGrant`.

// On top of the `WaitForGuarantees` endpoint, we then implement a Consumer of messages that respects guarantees. The `WaitForGuarantees` endpoint itself is not part of the exported interface.

use std::{cell::Cell, convert::Infallible, ops::DerefMut, rc::Rc};

use ufotofu::{BulkConsumer, Producer};

use either::Either;

use wb_async_utils::{Mutex, TakeCell};
use willow_encoding::{Encodable, EncodableExactSize, U16BE, U32BE, U64BE};

#[derive(Debug)]
pub struct SharedState {
    /// How many guarantees do we have available right now?
    guarantees_available: Cell<u64>,
    /// How many guarantees is the `WaitForGuarantees` endpoint waiting for right now, if for any?
    guarantees_threshold: Cell<Option<u64>>,
    /// How many more guarantees will we be granted at most, if we know a bound at all?
    guarantees_bound: Cell<Option<u64>>,
    /// Notify the `WaitForGuarantees` endpoint when enough guarantees are avaible (Ok) or
    /// we know that enough guarantees will never become available (Err).
    notify_guarantees: TakeCell<Result<(), ()>>,
    /// Notify the `AbsolutionsToGrant` endpoint when there *are* absolutions to grant.
    absolution_to_grant: TakeCell<u64>, // empty when no absolution to grant, inner u64 is always nonzero
}

/// Creates the three endpoint for managing the client-side state of a single logical channel.
fn new_logical_channel_client_state_internal() -> (Input, WaitForGuarantees, AbsolutionsToGrant) {
    let state = Rc::new(SharedState {
        guarantees_available: Cell::new(0),
        guarantees_threshold: Cell::new(None),
        guarantees_bound: Cell::new(None),
        notify_guarantees: TakeCell::new(),
        absolution_to_grant: TakeCell::new(),
    });

    return (
        Input {
            state: state.clone(),
        },
        WaitForGuarantees {
            state: state.clone(),
        },
        AbsolutionsToGrant { state: state },
    );
}

/// The endpoint for updating the client's state with information from the server.
pub struct Input {
    state: Rc<SharedState>,
}

impl Input {
    /// Updates the client state with more guarantees. To be called whenever receiving a [IssueGuarantee](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlGuarantee) message.
    ///
    /// Returns an `Err(())` when the available guarantees would exceed the maximum of `2^64 - 1`. If this happens, the logical channel should be considered completely unuseable (and usually, the whole connection should be dropped, since we are talking to a buggy or malicious peer).
    /// Silently ignores when the server sends guarantees exceeding a previously communicated bound.
    pub fn receive_guarantees(&mut self, amount: u64) -> Result<(), ()> {
        // By the way: we take `&mut self` instead of `&self` (which would also compile) because to the outside this function looks like it mutates some state.
        // The fact that all of that happens via interior mutability is a detail we don't need to communicate to the outside.

        // Can we increase the guarantees without an overflow?
        match self.state.guarantees_available.get().checked_add(amount) {
            // Nope, report an error.
            None => return Err(()),
            Some(new_amount) => {
                // Yes, so update guarantees.
                self.state.guarantees_available.set(new_amount);

                // Update the bound, if there is one.
                if let Some(bound) = self.state.guarantees_bound.get() {
                    // Using saturating subtraction (i.e., setting to zero if it would become negative)
                    // Means that we silently ignore that we got more guarantees than the bound promised.
                    // The server violated the protocol, but we don't want to spend resources on checking
                    // and handling this. We still will not make any *use* of those extra guarantees,
                    // because the `WaitForGuarantees` endpoint will receive an error once the bound
                    // has been reached.
                    let new_bound = bound.saturating_sub(amount);
                    self.state.guarantees_bound.set(Some(new_bound));

                    // No need to check the new bound against what the `WaitForGuarantees` endpoint requested,
                    // because the total of `guarantees_available + guarantees_bound` remained unchanged.
                }

                // Is anything currently waiting on having a certain threshold of guarantees?
                if let Some(threshold) = self.state.guarantees_threshold.get() {
                    if new_amount >= threshold {
                        // Yes, and we have that many. Tell them, then reduce the guarantees (we assume that whoever was waiting will send exactly that many bytes).
                        self.state.notify_guarantees.set(Ok(()));
                        self.state.guarantees_available.set(new_amount - threshold);
                    } else {
                        // Do nothing, we'll notify them later, once we got enough guarantees.
                    }
                }

                // We did all we had to do, and encountered no overflows. Report success.
                return Ok(());
            }
        }
    }

    /// Updates the client state after receiving a [`Plead`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOops) message with a given [`target`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOopsTarget) value. Fully accepts the `Plead`, if you want to reject some `Plead`s, you need a different implementation.
    pub fn receive_plead(&mut self, target: u64) {
        match self.state.guarantees_available.get().checked_sub(target) {
            None => {
                // Nothing to do if we already used up all the guarantees they ask us to absolve them off.
            }
            Some(diff) => {
                // Locally respect their wish for absolution.
                self.state.guarantees_available.set(target);

                // Store how many guarantees to absolve them off in the next absolution message, by adding the diff to what we stored previously (or using the diff directly, if we didn't store anything previously).
                // If this saturates, the peers will get out of sync... after sending 2^64 bytes, which they won't in all likelyhood.
                // And even if that happened, then it would be the other peer's fault, not ours.
                self.state
                    .absolution_to_grant
                    .update(|old| old.map_or(diff, |old_value| old_value.saturating_add(diff)));
            }
        }
    }

    /// Updates the client state after receiving a [`ControlLimitReceiving`](https://willowprotocol.org/specs/sync/index.html#ControlLimitReceiving) message with a given [`bound`](https://willowprotocol.org/specs/sync/index.html#ControlLimitReceivingBound) value.
    ///
    /// Silently ignores invalid bounds (i.e., bounds that are less tight than a previously communicated bound), and keeps using the older, tighter bound.
    pub fn receive_bound(&mut self, bound: u64) {
        let new_bound = match self.state.guarantees_bound.get() {
            None => {
                self.state.guarantees_bound.set(Some(bound));
                bound
            }
            Some(old) => {
                let new_bound = core::cmp::min(old, bound);
                self.state.guarantees_bound.set(Some(new_bound));
                new_bound
            }
        };

        if let Some(threshold) = self.state.guarantees_threshold.get() {
            if threshold
                > self
                    .state
                    .guarantees_available
                    .get()
                    .saturating_add(new_bound)
            {
                // We'll never get enough guarantees. Report the sad news to the `WaitForGuarantees` endpoint.
                self.state.notify_guarantees.set(Err(()));
            }
        }
    }
}

/// A [`local_nb::Producer`](ufotofu::local_nb::Producer) of amounts for `Absolve` messages that the client must send to the server.
///
/// The producer does not use the `Final` type to express when the stream has been closed so no absolutions can be granted in the future. Instead, this struct should be dropped when the corresponding `WaitForGuarantees` endpoint reports that no future guarantees will be granted.
pub struct AbsolutionsToGrant {
    state: Rc<SharedState>,
}

impl Producer for AbsolutionsToGrant {
    type Item = u64;

    type Final = Infallible;

    type Error = Infallible;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        return Ok(Either::Left(self.state.absolution_to_grant.take().await));
    }
}

/// A value for asynchronously waiting until a certain amount of guarantees is available.
pub struct WaitForGuarantees {
    state: Rc<SharedState>,
}

impl WaitForGuarantees {
    /// Pause until the desired amount of guarantees is available, then reduce the internal state by that amount. In other words, once the guarantees are available, they *must* be used up by sending messages of that side.
    pub async fn wait_for_guarantees(&mut self, amount: u64) -> Result<(), ()> {
        // Are sufficient guarantees available right now?
        let current_guarantees = self.state.guarantees_available.get();
        if current_guarantees >= amount {
            // If so, remove `amount` many, indicate that no more guarantees are awaited right now, and report success.
            self.state
                .guarantees_available
                .set(current_guarantees - amount);
            self.state.guarantees_threshold.set(None);
            return Ok(());
        } else {
            // Not enough guarantees here. Can that possibly change?
            if amount
                > current_guarantees.saturating_add(self.state.guarantees_bound.get().unwrap_or(0))
            {
                // We'll never get enough guarantees.
                return Err(());
            } else {
                // We might get enough guarantees in the future. Register our demand, and wait.
                self.state.guarantees_threshold.set(Some(amount));
                return self.state.notify_guarantees.take().await;
            }
        }
    }
}

/// A consumer that respects the guarantees available on a logical channel.
pub struct LogicalChannelClientEndpoint<'transport, C> {
    consumer: &'transport Mutex<C>,
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
    pub async fn send_to_channel<T: EncodableExactSize>(
        &mut self,
        message: T,
    ) -> Result<(), LogicalChannelClientError<C::Error>> {
        let size = message.encoded_size() as u64;

        // Compute how many bytes to encode the length in.
        let floored_base64_log_of_size = (size.ilog2() / 8) as u8;
        // Shift to the 2nd to 2th most significant bits to form the first four bits of the header byte.
        let mut first_byte = floored_base64_log_of_size << 3;
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
    let (input, wfg, atg) = new_logical_channel_client_state_internal();
    let lccc = LogicalChannelClientEndpoint {
        channel,
        guarantees: wfg,
        consumer,
    };

    return (input, lccc, atg);
}
