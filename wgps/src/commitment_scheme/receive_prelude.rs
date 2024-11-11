use either::Either;
use ufotofu::local_nb::BulkProducer;

/// When things go wrong while trying to make a WGPS transport ready.
#[derive(Debug)]
pub enum ReceivePreludeError<E> {
    /// The transport returned an error of its own.
    Transport(E),
    /// The received max payload power was invalid, i.e. greater than 64.
    MaxPayloadInvalid,
    /// The transport stopped producing bytes before it could be deemed ready.
    FinishedTooSoon,
}

impl<E: core::fmt::Display> core::fmt::Display for ReceivePreludeError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceivePreludeError::Transport(e) => write!(f, "{}", e),
            ReceivePreludeError::MaxPayloadInvalid => write!(
                f,
                "The received max payload power was invalid, i.e. greater than 64."
            ),
            ReceivePreludeError::FinishedTooSoon => write!(
                f,
                "The transport stopped producing bytes before it could be deemed ready."
            ),
        }
    }
}

/// The result of intercepting the first few bytes of a WGPS transport.
#[derive(Debug)]
#[allow(dead_code)] // TODO: Remove when this is used.
pub(crate) struct ReceivedPrelude<const CHALLENGE_HASH_LENGTH: usize> {
    /// The maximum payload size which may be sent without being explicitly requested.
    pub maximum_payload_size: usize,
    /// The challenge hash of a nonce.
    pub received_commitment: [u8; CHALLENGE_HASH_LENGTH],
}

/// Given a producer of bytes which is to immediately produce the bytes corresponding to the WGPS' [maximum payload size](https://willowprotocol.org/specs/sync/index.html#peer_max_payload_size) and [received commitment](https://willowprotocol.org/specs/sync/index.html#received_commitment), returns the computed maximum payload size, received commitment, and a 'ready' transport set to produce encoded WGPS messages.

#[allow(dead_code)] // TODO: Remove when this is used.
pub(crate) async fn receive_prelude<
    const CHALLENGE_HASH_LENGTH: usize,
    P: BulkProducer<Item = u8>,
>(
    transport: &mut P,
) -> Result<ReceivedPrelude<CHALLENGE_HASH_LENGTH>, ReceivePreludeError<P::Error>>
where
    P::Error: core::fmt::Display,
{
    let maximum_payload_power = match transport.produce().await? {
        Either::Left(byte) => byte,
        Either::Right(_) => return Err(ReceivePreludeError::FinishedTooSoon),
    };

    if maximum_payload_power > 64 {
        return Err(ReceivePreludeError::MaxPayloadInvalid);
    }

    let maximum_payload_size = 2_usize.pow(maximum_payload_power as u32);

    let mut received_commitment = [0_u8; CHALLENGE_HASH_LENGTH];

    if let Err(e) = transport
        .bulk_overwrite_full_slice(&mut received_commitment)
        .await
    {
        match e.reason {
            Either::Left(_) => return Err(ReceivePreludeError::FinishedTooSoon),
            Either::Right(e) => return Err(ReceivePreludeError::Transport(e)),
        }
    };

    Ok(ReceivedPrelude {
        maximum_payload_size,
        received_commitment,
    })
}

impl<E: core::fmt::Display> From<E> for ReceivePreludeError<E> {
    fn from(value: E) -> Self {
        ReceivePreludeError::Transport(value)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use ufotofu::local_nb::producer::FromSlice;

    #[test]
    fn empty_producer() {
        let mut empty_transport = FromSlice::new(&[]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut empty_transport).await;

            assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
        });
    }

    #[test]
    fn only_power_producer() {
        let mut only_power_transport = FromSlice::new(&[0_u8]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut only_power_transport).await;

            assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
        });
    }

    #[test]
    fn invalid_power_producer() {
        let mut only_power_transport = FromSlice::new(&[65_u8]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut only_power_transport).await;

            assert!(matches!(
                result,
                Err(ReceivePreludeError::MaxPayloadInvalid)
            ))
        });
    }

    #[test]
    fn invalid_power_producer_correct_length() {
        let mut only_power_transport = FromSlice::new(&[65_u8, 0, 0, 0, 0]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut only_power_transport).await;

            assert!(matches!(
                result,
                Err(ReceivePreludeError::MaxPayloadInvalid)
            ))
        });
    }

    #[test]
    fn commitment_too_short() {
        let mut only_power_transport = FromSlice::new(&[0_u8, 0]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut only_power_transport).await;

            assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
        });
    }

    #[test]
    fn success() {
        let mut only_power_transport = FromSlice::new(&[1_u8, 1, 2, 3, 4, 5]);

        smol::block_on(async {
            let result = receive_prelude::<4, _>(&mut only_power_transport).await;

            if let Ok(ready) = result {
                assert!(ready.maximum_payload_size == 2);
                assert!(ready.received_commitment == [1, 2, 3, 4]);
            } else {
                panic!()
            }
        });
    }
}
