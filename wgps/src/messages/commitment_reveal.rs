use ufotofu::BulkConsumer;
use willow_encoding::Encodable;

pub struct CommitmentReveal<'nonce, const CHALLENGE_LENGTH: usize> {
    pub nonce: &'nonce [u8; CHALLENGE_LENGTH],
}

impl<'nonce, const CHALLENGE_LENGTH: usize> Encodable
    for CommitmentReveal<'nonce, CHALLENGE_LENGTH>
{
    async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        // Encoding defined at https://willowprotocol.org/spec/sync/index.html#enc_commitment_reveal
        consumer.consume(0b1000_0000).await?;
        consumer
            .bulk_consume_full_slice(&self.nonce[..])
            .await
            .map_err(|err| err.reason)?;

        Ok(())
    }
}
