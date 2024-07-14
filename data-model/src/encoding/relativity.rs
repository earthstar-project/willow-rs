use std::future::Future;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        error::DecodeError,
        max_power::{decode_max_power, encode_max_power},
        parameters::{Decoder, Encoder},
    },
    path::{Path, PathRc},
};

/// A relationship representing a type `T` being encoded relative to type `R`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelativeEncoding<T, R>
where
    RelativeEncoding<T, R>: Encoder,
    R: RelativeDecoder<T>,
{
    /// The value we wish to encode or decode.
    pub subject: T,
    /// The reference value we are encoding the item relative to, usually known by whoever will decode the relative encoding.
    pub reference: R,
}

/// A type that can be used to decode from a bytestring *encoded relative to `Self`*.
pub trait RelativeDecoder<T> {
    /// A function from the set of bytestrings *encoded relative to `Self`* to the set of `T` in relation to `Self`.
    fn relative_decode<Producer>(
        &self,
        producer: &mut Producer,
    ) -> impl Future<Output = Result<T, DecodeError<Producer::Error>>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized;
}

// Path <> Path

impl<const MCL: usize, const MCC: usize, const MPL: usize>
    RelativeEncoding<PathRc<MCL, MCC, MPL>, PathRc<MCL, MCC, MPL>>
{
    pub fn new(path: PathRc<MCL, MCC, MPL>, reference: PathRc<MCL, MCC, MPL>) -> Self {
        RelativeEncoding {
            subject: path,
            reference,
        }
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> Encoder
    for RelativeEncoding<PathRc<MCL, MCC, MPL>, PathRc<MCL, MCC, MPL>>
{
    /// Encode a path relative to another path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn encode<Consumer>(
        &self,
        consumer: &mut Consumer,
    ) -> Result<(), super::error::EncodingConsumerError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let lcp = self.subject.longest_common_prefix(&self.reference);
        encode_max_power(lcp.component_count(), MCC, consumer).await?;

        let suffix = self
            .subject
            .create_suffix(self.subject.component_count() - lcp.component_count());
        suffix.encode(consumer).await?;

        Ok(())
    }
}

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeDecoder<PathRc<MCL, MCC, MPL>>
    for PathRc<MCL, MCC, MPL>
{
    /// Decode a path relative to this path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_decode<Producer>(
        &self,
        producer: &mut Producer,
    ) -> Result<Self, DecodeError<Producer::Error>>
    where
        Producer: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let lcp = decode_max_power(MCC, producer).await?;

        if lcp > self.component_count() as u64 {
            return Err(DecodeError::InvalidInput);
        }

        // What is this weird situation?
        // LCP is zero, but there IS overlap between

        let prefix = self.create_prefix(lcp as usize);
        let suffix = PathRc::<MCL, MCC, MPL>::decode(producer).await?;

        let mut new = prefix;

        for component in suffix.components() {
            match new.append(component.clone()) {
                Ok(appended) => new = appended,
                Err(_) => return Err(DecodeError::InvalidInput),
            }
        }

        let actual_lcp = self.longest_common_prefix(&new);

        if actual_lcp.component_count() != lcp as usize {
            return Err(DecodeError::InvalidInput);
        }

        Ok(new)
    }
}
