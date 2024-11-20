use ufotofu::local_nb::BulkConsumer;
use willow_encoding::Encodable;

pub struct CommitmentReveal<const CHALLENGE_LENGTH: usize> {
    pub nonce: [u8; CHALLENGE_LENGTH],
}

impl<const CHALLENGE_LENGTH: usize> Encodable for CommitmentReveal<CHALLENGE_LENGTH> {
    async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        consumer.consume(0x0).await?;
        consumer
            .bulk_consume_full_slice(&self.nonce[..])
            .await
            .map_err(|err| err.reason)?;

        Ok(())
    }
}
