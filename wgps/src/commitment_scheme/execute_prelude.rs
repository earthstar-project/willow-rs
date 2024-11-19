use futures::future::{select, try_select, Either};

// send our own commitment reveal message only after we have received the prelude.

// everything that needs the challenge has to wait until we have received their commitment reveal message.

use ufotofu::local_nb::{BulkConsumer, BulkProducer};
use willow_encoding::Encodable;

use crate::{ChallengeHash, CommitmentReveal};

use super::{
    receive_prelude::{receive_prelude, ReceivePreludeError, ReceivedPrelude},
    send_prelude::send_prelude,
};

pub enum ExecutePreludeError<E> {
    ReceiveError(ReceivePreludeError<E>),
    SendError(E),
}

impl<E> From<ReceivePreludeError<E>> for ExecutePreludeError<E> {
    fn from(value: ReceivePreludeError<E>) -> Self {
        ExecutePreludeError::ReceiveError(value)
    }
}

impl<E> From<E> for ExecutePreludeError<E> {
    fn from(value: E) -> Self {
        ExecutePreludeError::SendError(value)
    }
}

/// Given a consumer and producer, send a max payload size and commitment, and wait for the other side's corresponding `ReceivedPrelude`. Then send a `CommitmentReveal` message, before finally returning the received prelude.
///
/// Attention: waiting for the other peer's prelude means that we delay sending our first messages, even though technically we would be allowed to do that. PRs welcome.
pub(crate) async fn execute_prelude<
    const CHALLENGE_LENGTH: usize,
    const CHALLENGE_HASH_LENGTH: usize,
    CH: ChallengeHash<CHALLENGE_LENGTH, CHALLENGE_HASH_LENGTH>,
    E,
    C: BulkConsumer<Item = u8, Error = E>,
    P: BulkProducer<Item = u8, Error = E>,
>(
    max_payload_power: u8,
    our_nonce: [u8; CHALLENGE_LENGTH],
    consumer: &mut C,
    producer: &mut P,
) -> Result<ReceivedPrelude<CHALLENGE_HASH_LENGTH>, ExecutePreludeError<E>> {
    let commitment = CH::hash(our_nonce);

    let receive_fut = Box::pin(receive_prelude::<CHALLENGE_HASH_LENGTH, _>(producer));
    let send_fut = Box::pin(send_prelude(max_payload_power, commitment, consumer));

    let (received_prelude, ()) = match try_select(receive_fut, send_fut).await {
        Ok(Either::Left((received, send_fut))) => (received, send_fut.await?),
        Ok(Either::Right(((), receive_fut))) => (receive_fut.await?, ()),
        Err(Either::Left((error, _))) => return Err(error.into()),
        Err(Either::Right((error, _))) => return Err(error.into()),
    };

    let msg = CommitmentReveal { nonce: our_nonce };
    msg.encode(consumer).await?;

    Ok(received_prelude)
}
