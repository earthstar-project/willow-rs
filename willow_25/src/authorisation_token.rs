use compact_u64::{CompactU64, Tag, TagWidth};
use meadowcap::{
    AccessMode, CommunalCapability, Delegation, McAuthorisationToken, McCapability, OwnedCapability,
};
use ufotofu_codec::{
    Blame, Decodable, DecodeError, Encodable, EncodableKnownSize, EncodableSync, RelativeDecodable,
    RelativeEncodable, RelativeEncodableKnownSize,
};
use willow_data_model::{grouping::Area, AuthorisationToken, TrustedDecodable};
use willow_encoding::is_bitflagged;

#[derive(Clone, Debug)]
pub struct AuthorisationToken25(
    McAuthorisationToken<
        1024,
        1024,
        1024,
        crate::NamespaceId25,
        crate::Signature25,
        crate::SubspaceId25,
        crate::Signature25,
    >,
);

impl AuthorisationToken25 {
    pub fn new(
        capability: meadowcap::McCapability<
            1024,
            1024,
            1024,
            crate::NamespaceId25,
            crate::Signature25,
            crate::SubspaceId25,
            crate::Signature25,
        >,
        signature: crate::Signature25,
    ) -> Self {
        let token = McAuthorisationToken::new(capability, signature);

        Self(token)
    }

    pub fn capability(
        &self,
    ) -> &meadowcap::McCapability<
        1024,
        1024,
        1024,
        crate::NamespaceId25,
        crate::Signature25,
        crate::SubspaceId25,
        crate::Signature25,
    > {
        &self.0.capability
    }

    pub fn signature(&self) -> &crate::Signature25 {
        &self.0.signature
    }
}

impl
    AuthorisationToken<
        1024,
        1024,
        1024,
        crate::NamespaceId25,
        crate::SubspaceId25,
        crate::PayloadDigest25,
    > for AuthorisationToken25
{
    fn is_authorised_write(
        &self,
        entry: &willow_data_model::Entry<
            1024,
            1024,
            1024,
            crate::NamespaceId25,
            crate::SubspaceId25,
            crate::PayloadDigest25,
        >,
    ) -> bool {
        self.0.is_authorised_write(entry)
    }
}

impl Encodable for AuthorisationToken25 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: ufotofu::BulkConsumer<Item = u8>,
    {
        self.0.capability.relative_encode(consumer).await?;
        self.0.signature.encode(consumer).await?;

        Ok(())
    }
}

impl TrustedDecodable for AuthorisationToken25 {
    async unsafe fn trusted_decode<P>(
        producer: &mut P,
    ) -> Result<Self, ufotofu_codec::DecodeError<P::Final, P::Error, Blame>>
    where
        P: ufotofu::BulkProducer<Item = u8>,
    {
        let capability = McCapability::<
            1024,
            1024,
            1024,
            crate::NamespaceId25,
            crate::Signature25,
            crate::SubspaceId25,
            crate::Signature25,
        >::trusted_relative_decode(producer, Area::new_full())
        .await;
        let signature = crate::Signature25::decode(consumer).await?;

        Ok(Self(McAuthorisationToken {
            capability,
            signature,
        }))
    }
}
