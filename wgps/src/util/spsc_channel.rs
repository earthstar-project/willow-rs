use either::Either;
use std::{
    cell::RefCell,
    convert::Infallible,
    future::{ready, Future},
    rc::Rc,
    task::{Poll, Waker},
};
use ufotofu::local_nb::{
    BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer,
};
use ufotofu_queues::Queue;

struct SpscState<Q: Queue> {
    queue: Q,
    waker: Option<Waker>,
}

impl<Q: Queue> SpscState<Q> {}

pub(crate) struct Output<Q: Queue> {
    state: Rc<RefCell<SpscState<Q>>>,
}

struct ProduceFuture<'o, Q: Queue> {
    output: &'o Output<Q>,
}

impl<'o, Q: Queue> Future for ProduceFuture<'o, Q> {
    type Output = Result<Either<Q::Item, Infallible>, Infallible>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state_ref = self.output.state.borrow_mut();

        match state_ref.queue.dequeue() {
            Some(item) => Poll::Ready(Ok(Either::Left(item))),
            None => {
                state_ref.waker = Some(cx.waker().clone());

                Poll::Pending
            }
        }
    }
}

impl<Q: Queue> Producer for Output<Q> {
    type Item = Q::Item;

    type Final = Infallible;

    type Error = Infallible;

    fn produce(
        &mut self,
    ) -> impl Future<Output = Result<Either<Self::Item, Self::Final>, Self::Error>> {
        ProduceFuture { output: self }
    }
}

impl<Q: Queue> BufferedProducer for Output<Q> {
    fn slurp(&mut self) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }
}

// This is safe if and only if the object pointed at by `reference` lives for at least `'longer`.
// See https://doc.rust-lang.org/nightly/std/intrinsics/fn.transmute.html for more detail.
unsafe fn extend_lifetime<'shorter, 'longer, T: ?Sized>(reference: &'shorter T) -> &'longer T {
    std::mem::transmute::<&'shorter T, &'longer T>(reference)
}

struct ExposeItemsFuture<'o, Q: Queue> {
    output: &'o Output<Q>,
}

impl<'o, Q: Queue> Future for ExposeItemsFuture<'o, Q> {
    type Output = Result<Either<&'o [Q::Item], Infallible>, Infallible>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state_ref = self.output.state.borrow_mut();

        match state_ref.queue.expose_items() {
            Some(items) => {
                // You're probably wondering how I got here.
                // We need to return something which lives for 'o,
                // but we're borrowing it from this state_ref thing
                // which only lives as long as this function does.
                //
                // We *know* that these items will have a long enough lifetime,
                // because they sit inside a Rc which has a lifetime of 'o.
                // The whole point of the Rc is to keep its contents alive as long at least as itself.
                // Thus we know that the items have a lifetime of at least 'o.
                let items: &'o [Q::Item] = unsafe { extend_lifetime(items) };

                Poll::Ready(Ok(Either::Left(items)))
            }
            None => {
                state_ref.waker = Some(cx.waker().clone());

                Poll::Pending
            }
        }
    }
}

impl<Q: Queue> BulkProducer for Output<Q> {
    fn expose_items<'a>(
        &'a mut self,
    ) -> impl Future<Output = Result<Either<&'a [Self::Item], Self::Final>, Self::Error>>
    where
        Self::Item: 'a,
    {
        ExposeItemsFuture { output: self }
    }

    fn consider_produced(
        &mut self,
        amount: usize,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        let mut state_ref = self.state.borrow_mut();

        state_ref.queue.consider_dequeued(amount);

        ready(Ok(()))
    }
}

pub(crate) struct Input<Q: Queue> {
    state: Rc<RefCell<SpscState<Q>>>,
}

impl<Q: Queue> Input<Q> {
    pub fn consume(&mut self, item: Q::Item) -> Option<Q::Item> {
        let mut state_ref = self.state.borrow_mut();

        match state_ref.queue.enqueue(item) {
            Some(item) => Some(item),
            None => {
                if let Some(waker) = state_ref.waker.take() {
                    waker.wake_by_ref()
                };

                None
            }
        }
    }

    pub fn len(&self) -> usize {
        let state_ref = self.state.borrow();

        state_ref.queue.len()
    }
}

impl<Q: Queue> Consumer for Input<Q> {
    type Item = Q::Item;

    type Final = Infallible;

    type Error = Infallible;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        todo!()
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<Q: Queue> BufferedConsumer for Input<Q> {
    async fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<Q: Queue> BulkConsumer for Input<Q> {
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        todo!()
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        todo!()
    }
}
