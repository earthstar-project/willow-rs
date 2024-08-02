use syncify::syncify;

#[syncify(encoding_sync)]
pub mod encoding {

    use crate::encoding::error::DecodeError;

    use either::Either;
    use syncify::syncify_replace;

    #[syncify_replace(use ufotofu::sync::{ BulkProducer};)]
    use ufotofu::local_nb::BulkProducer;

    /// Have `Producer` produce a single byte, or return an error if the final value was produced or the producer experienced an error.
    pub async fn produce_byte<Producer>(
        producer: &mut Producer,
    ) -> Result<u8, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
    {
        match producer.produce().await {
            Ok(Either::Left(item)) => Ok(item),
            Ok(Either::Right(_)) => Err(DecodeError::InvalidInput),
            Err(err) => Err(DecodeError::Producer(err)),
        }
    }
}

pub fn is_bitflagged(byte: u8, position: u8) -> bool {
    let mask = match position {
        0 => 0b1000_0000,
        1 => 0b0100_0000,
        2 => 0b0010_0000,
        3 => 0b0001_0000,
        4 => 0b0000_1000,
        5 => 0b0000_0100,
        6 => 0b0000_0010,
        7 => 0b0000_0001,
        _ => panic!("Can't check for a bitflag at a position greater than 7"),
    };

    byte & mask == mask
}
