use ufotofu::ConsumeFullSliceError;
use willow_encoding::{Decodable, DecodeError, Encodable, RelationDecodable};

use either::Either::*;

/// The first few bytes to send/receive in a WGPS session.
#[derive(Debug, Clone)]
pub(crate) struct Prelude<const CHALLENGE_HASH_LENGTH: usize> {
    /// The base two logarithm of the maximum payload size that may be sent without being explicitly requested. Must be 64 or below.
    ///
    /// `Encode` ignores the two most significant bits. `Decode` yields an error if the two most significant bits are not zeros.
    pub max_payload_power: u8,
    /// The challenge hash of a nonce.
    pub commitment: [u8; CHALLENGE_HASH_LENGTH],
}

impl<const CHALLENGE_HASH_LENGTH: usize> Encodable for Prelude<CHALLENGE_HASH_LENGTH> {
    async fn encode<Consumer>(&self, consumer: &mut Consumer) -> Result<(), Consumer::Error>
    where
        Consumer: ufotofu::BulkConsumer<Item = u8>,
    {
        consumer.consume(self.max_payload_power).await?;

        if let Err(ConsumeFullSliceError { reason, .. }) =
            consumer.bulk_consume_full_slice(&self.commitment[..]).await
        {
            return Err(reason);
        }

        // FIXME use this instead of the above if let after updating ufotofu
        // consumer
        //     .bulk_consume_full_slice(&self.commitment[..])
        //     .await
        //     .map_err(|err| err.into())?;
        Ok(())
    }
}

impl<const CHALLENGE_HASH_LENGTH: usize> Decodable for Prelude<CHALLENGE_HASH_LENGTH> {
    async fn decode_canonical<Producer>(
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        let max_payload_power = match producer
            .produce()
            .await
            .map_err(|err| DecodeError::Producer(err))?
        {
            Left(byte) => byte,
            Right(_) => return Err(DecodeError::InvalidInput),
        };

        if max_payload_power > 64 {
            return Err(DecodeError::InvalidInput);
        }

        let mut commitment = [0_u8; CHALLENGE_HASH_LENGTH];

        if let Err(e) = producer.bulk_overwrite_full_slice(&mut commitment).await {
            match e.reason {
                Left(_) => return Err(DecodeError::InvalidInput),
                Right(e) => return Err(DecodeError::Producer(e)),
            }
        };

        Ok(Prelude {
            max_payload_power,
            commitment,
        })
    }
}

impl<const CHALLENGE_HASH_LENGTH: usize> RelationDecodable for Prelude<CHALLENGE_HASH_LENGTH> {}

// TODO enable tests again
// TODO at least one encoding test

// #[cfg(test)]
// mod tests {

//     use super::*;

//     use ufotofu::local_nb::producer::FromSlice;

//     #[test]
//     fn decode_empty_producer() {
//         let mut empty_transport = FromSlice::new(&[]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut empty_transport).await;

//             assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
//         });
//     }

//     #[test]
//     fn decode_only_power_producer() {
//         let mut only_power_transport = FromSlice::new(&[0_u8]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut only_power_transport).await;

//             assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
//         });
//     }

//     #[test]
//     fn decode_invalid_power_producer() {
//         let mut only_power_transport = FromSlice::new(&[65_u8]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut only_power_transport).await;

//             assert!(matches!(
//                 result,
//                 Err(ReceivePreludeError::MaxPayloadInvalid)
//             ))
//         });
//     }

//     #[test]
//     fn decode_invalid_power_producer_correct_length() {
//         let mut only_power_transport = FromSlice::new(&[65_u8, 0, 0, 0, 0]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut only_power_transport).await;

//             assert!(matches!(
//                 result,
//                 Err(ReceivePreludeError::MaxPayloadInvalid)
//             ))
//         });
//     }

//     #[test]
//     fn decode_commitment_too_short() {
//         let mut only_power_transport = FromSlice::new(&[0_u8, 0]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut only_power_transport).await;

//             assert!(matches!(result, Err(ReceivePreludeError::FinishedTooSoon)))
//         });
//     }

//     #[test]
//     fn decode_success() {
//         let mut only_power_transport = FromSlice::new(&[1_u8, 1, 2, 3, 4, 5]);

//         smol::block_on(async {
//             let result = receive_prelude::<4, _>(&mut only_power_transport).await;

//             if let Ok(ready) = result {
//                 assert!(ready.maximum_payload_size == 2);
//                 assert!(ready.received_commitment == [1, 2, 3, 4]);
//             } else {
//                 panic!()
//             }
//         });
//     }
// }
