use meadowcap::McAuthorisationToken;
use willow_data_model::AuthorisationToken;

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
