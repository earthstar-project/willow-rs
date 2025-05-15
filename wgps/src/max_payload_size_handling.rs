use either::Either::{self, Left, Right};
use ufotofu::{BufferedConsumer, BufferedProducer, BulkConsumer, BulkProducer, Consumer, Producer};
use wb_async_utils::OnceCell;

use crate::WgpsError;

pub struct ConsumerButFirstSendByte<C> {
    byte: Option<u8>,
    c: C,
}

impl<C> ConsumerButFirstSendByte<C> {
    pub fn new(byte: u8, c: C) -> Self {
        Self {
            byte: Some(byte),
            c,
        }
    }
}

impl<C> Consumer for ConsumerButFirstSendByte<C>
where
    C: Consumer<Item = u8>,
{
    type Item = u8;

    type Final = C::Final;

    type Error = C::Error;

    async fn consume(&mut self, item: Self::Item) -> Result<(), Self::Error> {
        if let Some(byte) = self.byte.take() {
            self.c.consume(byte).await?;
        }

        self.c.consume(item).await
    }

    async fn close(&mut self, fin: Self::Final) -> Result<(), Self::Error> {
        if let Some(byte) = self.byte.take() {
            self.c.consume(byte).await?;
        }

        self.close(fin).await
    }
}

impl<C> BufferedConsumer for ConsumerButFirstSendByte<C>
where
    C: BufferedConsumer<Item = u8>,
{
    async fn flush(&mut self) -> Result<(), Self::Error> {
        if let Some(byte) = self.byte.take() {
            self.c.consume(byte).await?;
        }

        self.c.flush().await
    }
}

impl<C> BulkConsumer for ConsumerButFirstSendByte<C>
where
    C: BulkConsumer<Item = u8>,
{
    async fn expose_slots<'a>(&'a mut self) -> Result<&'a mut [Self::Item], Self::Error>
    where
        Self::Item: 'a,
    {
        if let Some(byte) = self.byte.take() {
            self.c.consume(byte).await?;
        }

        self.c.expose_slots().await
    }

    async fn consume_slots(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.c.consume_slots(amount).await
    }
}

pub struct ConsumerButFirstSetReceivedMaxPayloadSize<'cell, P> {
    received_max_payload_size: Option<&'cell OnceCell<u64>>,
    p: P,
}

impl<'cell, P> ConsumerButFirstSetReceivedMaxPayloadSize<'cell, P>
where
    P: Producer<Item = u8>,
{
    pub fn new(received_max_payload_size: &'cell OnceCell<u64>, p: P) -> Self {
        Self {
            received_max_payload_size: Some(received_max_payload_size),
            p,
        }
    }

    async fn first_set_received_max_payload_size(
        &mut self,
    ) -> Result<Either<(), P::Final>, WgpsError<P::Error>> {
        if let Some(cell) = self.received_max_payload_size.take() {
            match self.p.produce().await? {
                Right(fin) => return Ok(Right(fin)),
                Left(byte) => {
                    if byte > 64 {
                        return Err(WgpsError::InvalidMaxPayloadSize);
                    } else if byte == 64 {
                        let _ = cell.set(u64::MAX);
                    } else {
                        let _ = cell.set(1 << byte);
                    }
                }
            }
        }

        Ok(Left(()))
    }
}

impl<'cell, P> Producer for ConsumerButFirstSetReceivedMaxPayloadSize<'cell, P>
where
    P: Producer<Item = u8>,
{
    type Item = u8;

    type Final = P::Final;

    type Error = WgpsError<P::Error>;

    async fn produce(&mut self) -> Result<Either<Self::Item, Self::Final>, Self::Error> {
        if let Right(fin) = self.first_set_received_max_payload_size().await? {
            return Ok(Right(fin));
        }

        Ok(self.p.produce().await?)
    }
}

impl<'cell, P> BufferedProducer for ConsumerButFirstSetReceivedMaxPayloadSize<'cell, P>
where
    P: BufferedProducer<Item = u8>,
{
    async fn slurp(&mut self) -> Result<(), Self::Error> {
        let _ = self.first_set_received_max_payload_size().await?;

        Ok(self.p.slurp().await?)
    }
}

impl<'cell, P> BulkProducer for ConsumerButFirstSetReceivedMaxPayloadSize<'cell, P>
where
    P: BulkProducer<Item = u8>,
{
    async fn expose_items<'a>(
        &'a mut self,
    ) -> Result<Either<&'a [Self::Item], Self::Final>, Self::Error>
    where
        Self::Item: 'a,
    {
        if let Right(fin) = self.first_set_received_max_payload_size().await? {
            return Ok(Right(fin));
        }

        Ok(self.p.expose_items().await?)
    }

    async fn consider_produced(&mut self, amount: usize) -> Result<(), Self::Error> {
        Ok(self.p.consider_produced(amount).await?)
    }
}
