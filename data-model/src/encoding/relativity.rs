use std::future::Future;
use ufotofu::local_nb::{BulkConsumer, BulkProducer};

use crate::{
    encoding::{
        error::{DecodeError, EncodingConsumerError},
        max_power::{decode_max_power, encode_max_power},
        parameters::{Decoder, Encoder},
    },
    path::{Path, PathRc},
};

/// Returned when a relative encoding fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeEncodeError<E> {
    /// The subject could not be encoded relative to the reference (e.g. because it was not logically included by the reference).
    IllegalRelativity(),
    /// The encoding failed to be consumed by a [`ufotofu::local_nb::Consumer`].
    Consumer(EncodingConsumerError<E>),
}

impl<E> From<EncodingConsumerError<E>> for RelativeEncodeError<E> {
    fn from(err: EncodingConsumerError<E>) -> Self {
        RelativeEncodeError::Consumer(err)
    }
}

/// A type that can be used to encoded to a bytestring *encoded relative to `R`*.
pub trait RelativeEncoder<R> {
    /// A function from the set `Self` to the set of bytestrings *encoded relative to `reference`*.
    fn relative_encode<Consumer>(
        &self,
        reference: &R,
        consumer: &mut Consumer,
    ) -> impl Future<Output = Result<(), RelativeEncodeError<Consumer::Error>>>
    where
        Consumer: BulkConsumer<Item = u8>;
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

impl<const MCL: usize, const MCC: usize, const MPL: usize> RelativeEncoder<PathRc<MCL, MCC, MPL>>
    for PathRc<MCL, MCC, MPL>
{
    /// Encode a path relative to another path.
    ///
    /// [Definition](https://willowprotocol.org/specs/encodings/index.html#enc_path_relative)
    async fn relative_encode<Consumer>(
        &self,
        reference: &PathRc<MCL, MCC, MPL>,
        consumer: &mut Consumer,
    ) -> Result<(), RelativeEncodeError<Consumer::Error>>
    where
        Consumer: BulkConsumer<Item = u8>,
    {
        let lcp = self.longest_common_prefix(reference);
        encode_max_power(lcp.component_count(), MCC, consumer).await?;

        let suffix = self.create_suffix(self.component_count() - lcp.component_count());
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
