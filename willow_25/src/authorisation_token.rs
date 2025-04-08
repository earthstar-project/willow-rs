use meadowcap::McAuthorisationToken;
use willow_data_model::AuthorisationToken;

/// A [`McCapability`](https://willowprotocol.org/specs/meadowcap/index.html#Capability) configured with Willowʼ25 parameters.
pub type Capability25 = meadowcap::McCapability<
    1024,
    1024,
    1024,
    crate::NamespaceId25,
    crate::Signature25,
    crate::SubspaceId25,
    crate::Signature25,
>;

/// A [`meadowcap::McAuthorisationToken`] suitable for the Willow Data Model's [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) parameter.
///
/// These tokens use the [`Meadowcap`](https://willowprotocol.org/specs/meadowcap/index.html#meadowcap) capability system, and can be used to authorise a user's capability to write a given entry to a namespace.
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
    /// Returns a new [`AuthorisationToken25`] for a given [`meadowcap::McCapability`] and [`Signature25`].
    pub fn new(capability: Capability25, signature: crate::Signature25) -> Self {
        let token = McAuthorisationToken::new(capability, signature);

        Self(token)
    }

    /// Returns a reference to the inner [`Capability25`].
    pub fn capability(&self) -> &Capability25 {
        &self.0.capability
    }

    /// Returns a reference to the inner [`Signature25`].
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
