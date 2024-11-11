use futures::future::{select, Either};

// send our own commitment reveal message only after we have received the prelude.

// everything that needs the challenge has to wait until we have received their commitment reveal message.

use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use super::{receive_prelude::receive_prelude, send_prelude::send_prelude};

async fn execute_prelude<
    const CHALLENGE_HASH_LENGTH: usize,
    C: BulkConsumer<Item = u8>,
    P: BulkProducer<Item = u8>,
>(
    max_payload_size: u8,
    commitment: [u8; CHALLENGE_HASH_LENGTH],
    consumer: &mut C,
    producer: &mut P,
) {
    // This should be an async block doing more!
    // Should also send a commitment reveal message after having received the prelude.
    let receive_fut = Box::pin(receive_prelude::<CHALLENGE_HASH_LENGTH, _>(producer));
    let send_fut = Box::pin(send_prelude(max_payload_size, commitment, consumer));

    match select(receive_fut, send_fut).await {
        Either::Left((Ok(received), mut send_fut)) => todo!(),
        Either::Left((Err(error), mut send_fut)) => todo!(),
        Either::Right((Err(error), mut receive_fut)) => todo!(),
        Either::Right((Ok(()), mut receive_fut)) => todo!(),
    }
}
