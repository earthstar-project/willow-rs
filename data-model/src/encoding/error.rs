/// Everything that can go wrong when decoding a value.
#[derive(Debug)]
pub enum DecodeError<ProducerError> {
    Producer(ProducerError),
    InvalidInput,
}
