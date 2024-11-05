use either::Either;
use ufotofu::local_nb::BulkProducer;

/** When things go wrong while trying to make a WGPS transport ready. */
#[derive(Debug)]
pub enum ReadyTransportError<E: core::fmt::Display> {
    /** The transport returned an error of its own. */
    Transport(E),
    /** The received max payload power was invalid, i.e. greater than 64. */
    MaxPayloadInvalid,
    /** The transport stopped producing bytes before it could be deemed ready. */
    FinishedTooSoon,
}

impl<E: core::fmt::Display> core::fmt::Display for ReadyTransportError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadyTransportError::Transport(e) => write!(f, "{}", e),
            ReadyTransportError::MaxPayloadInvalid => write!(
                f,
                "The received max payload power was invalid, i.e. greater than 64."
            ),
            ReadyTransportError::FinishedTooSoon => write!(
                f,
                "The transport stopped producing bytes before it could be deemed ready."
            ),
        }
    }
}

/** The result of intercepting the first few bytes of a WGPS transport. */
#[derive(Debug)]
pub(crate) struct ReadyTransport<
    const CHALLENGE_HASH_LENGTH: usize,
    E: core::fmt::Display,
    P: BulkProducer<Item = u8, Final = (), Error = E>,
> {
    /** The maximum payload size which may be sent without being explicitly requested.*/
    pub maximum_payload_size: usize,
    /** The challenge hash of a nonce. */
    pub received_commitment: [u8; CHALLENGE_HASH_LENGTH],
    /** A 'ready' transport set to immediately produce encoded WGPS messages. */
    pub transport: P,
}

/** Given a producer of bytes which is to immediately produce the bytes corresponding to the WGPS' [maximum payload size](https://willowprotocol.org/specs/sync/index.html#peer_max_payload_size) and [received commitment](https://willowprotocol.org/specs/sync/index.html#received_commitment), returns the computed maximum payload size, received commitment, and a 'ready' transport set to produce encoded WGPS messages.
*/
pub(crate) async fn ready_transport<
    const CHALLENGE_HASH_LENGTH: usize,
    E: core::fmt::Display,
    P: BulkProducer<Item = u8, Final = (), Error = E>,
>(
    mut transport: P,
) -> Result<ReadyTransport<CHALLENGE_HASH_LENGTH, E, P>, ReadyTransportError<E>> {
    let maximum_payload_power = match transport.produce().await {
        Ok(either) => match either {
            Either::Left(byte) => Ok(byte),
            Either::Right(_) => Err(ReadyTransportError::FinishedTooSoon),
        },
        Err(transport_err) => Err(ReadyTransportError::Transport(transport_err)),
    }?;

    if maximum_payload_power > 64 {
        return Err(ReadyTransportError::MaxPayloadInvalid);
    }

    let maximum_payload_size = 2_usize.pow(maximum_payload_power as u32);

    let mut received_commitment = [0_u8; CHALLENGE_HASH_LENGTH];

    if let Err(e) = transport
        .bulk_overwrite_full_slice(&mut received_commitment)
        .await
    {
        match e.reason {
            Either::Left(_) => return Err(ReadyTransportError::FinishedTooSoon),
            Either::Right(e) => return Err(ReadyTransportError::Transport(e)),
        }
    };

    Ok(ReadyTransport {
        maximum_payload_size,
        received_commitment,
        transport,
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    use ufotofu::local_nb::producer::FromSlice;

    #[test]
    fn empty_producer() {
        let empty_transport = FromSlice::new(&[]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(empty_transport).await;

            assert!(matches!(result, Err(ReadyTransportError::FinishedTooSoon)))
        });
    }

    #[test]
    fn only_power_producer() {
        let only_power_transport = FromSlice::new(&[0_u8]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(only_power_transport).await;

            assert!(matches!(result, Err(ReadyTransportError::FinishedTooSoon)))
        });
    }

    #[test]
    fn invalid_power_producer() {
        let only_power_transport = FromSlice::new(&[65_u8]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(only_power_transport).await;

            assert!(matches!(
                result,
                Err(ReadyTransportError::MaxPayloadInvalid)
            ))
        });
    }

    #[test]
    fn invalid_power_producer_correct_length() {
        let only_power_transport = FromSlice::new(&[65_u8, 0, 0, 0, 0]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(only_power_transport).await;

            assert!(matches!(
                result,
                Err(ReadyTransportError::MaxPayloadInvalid)
            ))
        });
    }

    #[test]
    fn commitment_too_short() {
        let only_power_transport = FromSlice::new(&[0_u8, 0]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(only_power_transport).await;

            assert!(matches!(result, Err(ReadyTransportError::FinishedTooSoon)))
        });
    }

    #[test]
    fn success() {
        let only_power_transport = FromSlice::new(&[1_u8, 1, 2, 3, 4, 5]);

        smol::block_on(async {
            let result = ready_transport::<4, _, _>(only_power_transport).await;

            if let Ok(ready) = result {
                assert!(ready.maximum_payload_size == 2);
                assert!(ready.received_commitment == [1, 2, 3, 4]);
            } else {
                panic!()
            }
        });
    }
}
