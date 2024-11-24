use ufotofu::{local_nb::BulkConsumer, nb::ConsumeFullSliceError};

/// Sends the prelude of maximum payload power and what is to be the recipient's `received_commitment` via a transport.
pub(crate) async fn send_prelude<const CHALLENGE_HASH_LENGTH: usize, C: BulkConsumer<Item = u8>>(
    max_payload_power: u8,
    commitment: &[u8; CHALLENGE_HASH_LENGTH],
    transport: &mut C,
) -> Result<(), C::Error> {
    transport.consume(max_payload_power).await?;

    if let Err(ConsumeFullSliceError { reason, .. }) =
        transport.bulk_consume_full_slice(commitment).await
    {
        return Err(reason);
    }

    Ok(())
}
