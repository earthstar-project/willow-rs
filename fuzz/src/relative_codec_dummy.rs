//! Provides a type that implements RelativeEncodable and RelativeDEcodable and their subtraits. Simply a wrpaper around u16 that encodes relative to another u16 by taking the XOR.

use std::convert::Infallible;

use arbitrary::Arbitrary;
use ufotofu_codec::{
    Decodable, Encodable, RelativeDecodable, RelativeDecodableCanonic, RelativeDecodableSync,
    RelativeEncodable, RelativeEncodableKnownSize, RelativeEncodableSync,
};
use ufotofu_codec_endian::U16BE;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Arbitrary)]
pub struct XORU16(pub u16);

impl From<u16> for XORU16 {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<XORU16> for u16 {
    fn from(value: XORU16) -> Self {
        value.0
    }
}

impl RelativeEncodable<u16> for XORU16 {
    async fn relative_encode<C>(&self, consumer: &mut C, r: &u16) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        let xor = self.0 ^ *r;
        U16BE(xor).encode(consumer).await
    }
}

impl RelativeEncodableKnownSize<u16> for XORU16 {
    fn relative_len_of_encoding(&self, _r: &u16) -> usize {
        2
    }
}

impl RelativeEncodableSync<u16> for XORU16 {}

impl RelativeDecodable<u16, Infallible> for XORU16 {
    async fn relative_decode<P>(
        producer: &mut P,
        r: &u16,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Infallible>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        Ok(XORU16(U16BE::decode(producer).await?.0 ^ *r))
    }
}

impl RelativeDecodableCanonic<u16, Infallible, Infallible> for XORU16 {
    async fn relative_decode_canonic<P>(
        producer: &mut P,
        r: &u16,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Infallible>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
        Self: Sized,
    {
        Self::relative_decode(producer, r).await
    }
}

impl RelativeDecodableSync<u16, Infallible> for XORU16 {}
