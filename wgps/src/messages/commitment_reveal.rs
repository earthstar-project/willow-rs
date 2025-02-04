use lcmux::{GetGlobalNibble, SendGlobalNibble};
use ufotofu::BulkConsumer;
use ufotofu_codec::RelativeEncodable;

pub struct CommitmentReveal<'nonce, const CHALLENGE_LENGTH: usize> {
    pub nonce: &'nonce [u8; CHALLENGE_LENGTH],
}

impl<'nonce, const CHALLENGE_LENGTH: usize> RelativeEncodable<SendGlobalNibble>
    for CommitmentReveal<'nonce, CHALLENGE_LENGTH>
{
    async fn relative_encode<C>(
        &self,
        consumer: &mut C,
        _r: &SendGlobalNibble,
    ) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        consumer
            .bulk_consume_full_slice(&self.nonce[..])
            .await
            .map_err(|err| err.reason)
    }
}

impl<'nonce, const CHALLENGE_LENGTH: usize> GetGlobalNibble
    for CommitmentReveal<'nonce, CHALLENGE_LENGTH>
{
    fn control_nibble(&self) -> SendGlobalNibble {
        0b0000_0000
    }
}
