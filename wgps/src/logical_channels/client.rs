//! Components for the client side of logical channels, i.e., the side that receives guarantees and respects them. This implementation always respects the granted guarantees, it does not allow for optimistic sending, and has no way of handling the communicaiton that can be caused by overly optimistic sending.
//!
//! For each logical channel to write to, the client tracks the available guarantees at any point in time. It further tracks incoming `Plead` messages. This implementation fully honours all `Plead` messages.
//!
//! Note that this module does not deal with any sort of message encoding or decoding, it merely provides the machinery for tracking and modifying guarantees.

// Implementing correct client behaviour requires working both with incoming data and sending data to an outgoing channel. Both happen concurrently, and both need to be able to modify the state that the client must track. Since rust is not keen on shared mutable state, we follow the common pattern of having a shared state in a RefCell inside an Rc, with multiple values having access to the Rc. See also https://rust-lang.github.io/async-book/02_execution/03_wakeups.html for an example.

// We have three access points to the mutable state:
//
// - A receiving end that synchronously updates the state based on incoming `IssueGuarantee` and `Plead` messages (there are no facilities for processing `Apologise` messages, since this implementation never causes them): `Input`.
// - An end for sending messages on the logical channel: the program can asynchronously wait for a notification that a certain amount of credit has become available; this credit is considered to be used up once the async notification has been delivered: `WaitForGuarantees`.
// - An end for asynchronously listening for received `Plead` messages. This component generates information about what sort of `Absolve` messages the client must then send to keep everything consistent. Internally, every `Plead` message is immediately respected (i.e., the `WaitForGuarantees` endpoint will have to wait longer): `AbsolutionsToGrant`.

use std::{
    cell::RefCell,
    convert::Infallible,
    future::Future,
    rc::Rc,
    task::{Poll, Waker},
};

use either::Either;
use ufotofu::local_nb::Producer;

pub fn new_logical_channel_client_state() -> (Input, WaitForGuarantees, AbsolutionsToGrant) {
    unimplemented!()
}

struct SharedState {
    /// How many guarantees are available to the client at the current point in time?
    available_guarantees: u64,
    absolution_to_grant_amount: u64,
    request_guarantees_waker: Option<(
        Waker,
        u64, /* how many guarantees must be available to wake the waker? */
    )>,
    absolution_to_grant_waker: Option<Waker>,
}

/// The endpoint for updating the client's state with information from the server.
pub struct Input {
    state: Rc<RefCell<SharedState>>,
}

impl Input {
    /// Updates the client state with more guarantees. To be called whenever receiving a [IssueGuarantee](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlGuarantee) message.
    ///
    /// Returns an `Err(())` when the available guarantees would exceed the maximum of `2^64 - 1`. If this happens, the logical channel should be considered completely unuseable (and usually, the whole connection should be dropped, since we are talking to a buggy or malicious peer).
    pub fn receive_guarantees(&mut self, amount: u64) -> Result<(), ()> {
        // By the way: we take `&mut self` instead of `&self` (which would also compile) because to the outside this function looks like it mutates some state.
        // The fact that all of that happens via interiour mutability is a detail we don't need to communicate to the outside.

        let mut state_ref = self.state.borrow_mut();

        // Can we increase the guarantees without an overflow?
        match state_ref.available_guarantees.checked_add(amount) {
            // Nope, report an error.
            None => return Err(()),
            Some(new_amount) => {
                // Yes, so update guarantees
                state_ref.available_guarantees = new_amount;

                // Is anything cucrently waiting on having a certain threshold of guarantees?
                if let Some((waker, threshold)) = state_ref.request_guarantees_waker.take() {
                    if state_ref.available_guarantees >= threshold {
                        // Yes, and we have that many. Wake them, then reduce the guarantees (we assume that whoever was waiting will send exactly that many bytes).
                        waker.wake();
                        state_ref.available_guarantees -= threshold;
                    } else {
                        // Somebody was waking, but we don't have enough guarantees yet, so they will have to keep waiting.
                        state_ref.request_guarantees_waker = Some((waker, threshold));
                    }
                }
                // If nobody was waiting, then there is nothing to do.

                // We did all we had to do, and encountered no overflows. Report success.
                return Ok(());
            }
        }
    }

    /// Updates the client state after receiving a [`Plead`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOops) message with a given [`target`](https://willowprotocol.org/specs/resource-control/index.html#ResourceControlOopsTarget) value. Fully accepts the `Plead`, if you want to reject some `Plead`s, you need a different implementation.
    ///
    /// Returns an `Err(())` if the accumulated amount to absolve the server off would exceed `2^64 - 1` (it is possible to receive several Plead messages before getting to send an Absolve message, but we don't want unbounded space requirements, so we error instead - this should never happen with an honest, non-buggy server).
    pub fn receive_plead(&mut self, target: u64) -> Result<(), ()> {
        let mut state_ref = self.state.borrow_mut();

        // Nothing to do if we already used up all the guarantees they ask us to absolve them off.
        if state_ref.available_guarantees <= target {
            return Ok(());
        } else {
            // How much do we absolve them off?
            let diff = state_ref.available_guarantees - target;

            // Locally respect their wish for absolution.
            state_ref.available_guarantees = target;

            // Store how many guarantees to absolve them off in the next absolution message, by adding the diff to what we stored previously.
            // This can overflow, in which case we error.
            match state_ref.absolution_to_grant_amount.checked_add(diff) {
                None => return Err(()),
                Some(new_total) => state_ref.absolution_to_grant_amount = new_total,
            }

            // Wake anyone who was waiting for the granting of absolution.
            // They will then read the value in `state_ref.absolution_to_grant_amount` to put into an `Absolve` message.
            // They will also reset `state_ref.absolution_to_grant_amount` to zero when they do.
            if let Some(waker) = state_ref.absolution_to_grant_waker.take() {
                waker.wake();
            }

            // All done. Yay!
            return Ok(());
        }
    }
}

/// A [`local_nb::Producer`](ufotofu::local_nb::Producer) of amounts for `Absolve` messages that the client must send to the server.
pub struct AbsolutionsToGrant {
    state: Rc<RefCell<SharedState>>,
}

impl Producer for AbsolutionsToGrant {
    type Item = u64;

    type Final = Infallible;

    type Error = Infallible;

    fn produce(
        &mut self,
    ) -> impl Future<Output = Result<Either<Self::Item, Self::Final>, Self::Error>> {
        WaitForNonzeroAbsolutionFuture { atg: self }
    }
}

struct WaitForNonzeroAbsolutionFuture<'atg> {
    atg: &'atg AbsolutionsToGrant,
}

impl<'atg> Future for WaitForNonzeroAbsolutionFuture<'atg> {
    type Output = Result<Either<u64, Infallible>, Infallible>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state_ref = self.atg.state.borrow_mut();

        // Are there pending absolutions to grant right now?
        if state_ref.absolution_to_grant_amount == 0 {
            // No, so register the task to be notified once there are.
            state_ref.absolution_to_grant_waker = Some(cx.waker().clone());
            Poll::Pending
        } else {
            // Yes, we can send an `Absolve` message. Reset the internal counter and report the amount.
            let amount = state_ref.absolution_to_grant_amount;
            state_ref.absolution_to_grant_amount = 0;
            Poll::Ready(Ok(Either::Left(amount)))
        }
    }
}

/// A value for asynchronously waiting until a certain amount of guarantees is available.
pub struct WaitForGuarantees {
    state: Rc<RefCell<SharedState>>,
}

impl WaitForGuarantees {
    /// Pause until the desired amount of guarantees is available, then reduce the internal state by that amount. In other words, once the guarantees are available, they *must* be used up by sending messages of that side.
    pub fn wait_for_guarantees<'wfg>(
        &'wfg mut self,
        amount: u64,
    ) -> WaitForGuarannteesFuture<'wfg> {
        WaitForGuarannteesFuture { wfg: self, amount }
    }
}

pub struct WaitForGuarannteesFuture<'wfg> {
    wfg: &'wfg WaitForGuarantees,
    amount: u64,
}

impl<'wfg> Future for WaitForGuarannteesFuture<'wfg> {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state_ref = self.wfg.state.borrow_mut();

        // Are there enough guarantees available right now?
        if state_ref.available_guarantees >= self.amount {
            // Yes. Reduce the available guarantees and yield successfully.
            state_ref.available_guarantees -= self.amount;
            Poll::Ready(())
        } else {
            // No, so park the task, together with the requested amount, so that the corresponding `Input` knows when to wake it.
            state_ref.request_guarantees_waker = Some((cx.waker().clone(), self.amount));
            Poll::Pending
        }
    }
}

// TODO rewrite based on `async_cell::unsync::AsyncCell`
// TODO must also handle channel closing
// TODO provide a Consumer for items that can be encoded with known size
