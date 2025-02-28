use signature::Verifier;
use ufotofu_codec::{EncodableKnownSize, EncodableSync};
use willow_data_model::{AuthorisationToken, Entry, PayloadDigest};

use crate::{mc_capability::McCapability, AccessMode, McNamespacePublicKey, McPublicUserKey};

#[cfg(feature = "dev")]
use crate::{SillyPublicKey, SillySig};

/// To be used as the [`AuthorisationToken`](https://willowprotocol.org/specs/data-model/index.html#AuthorisationToken) parameter for the [Willow data model](https://willowprotocol.org/specs/data-model).
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#MeadowcapAuthorisationToken)
#[derive(Debug, Clone)]
pub struct McAuthorisationToken<
    const MCL: usize,
    const MCC: usize,
    const MPL: usize,
    NamespacePublicKey,
    NamespaceSignature,
    UserPublicKey,
    UserSignature,
> where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    /// Certifies that an Entry may be written.
    pub capability: McCapability<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >,
    /// Proves that the [`willow_data_model::Entry`] was created by the [receiver](https://willowprotocol.org/specs/meadowcap/index.html#cap_receiver) of the [capability](https://willowprotocol.org/specs/meadowcap/index.html#mcat_cap).
    pub signature: UserSignature,
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
    McAuthorisationToken<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
{
    /// Returns a new [`McAuthorisationToken`] using the given [`McCapability`] and [`UserSignature`].
    ///
    /// Does **not** verify the signature's validity.
    pub fn new(
        capability: McCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        signature: UserSignature,
    ) -> Self {
        Self {
            capability,
            signature,
        }
    }
}

impl<
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
        PD,
    > AuthorisationToken<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>
    for McAuthorisationToken<
        MCL,
        MCC,
        MPL,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    >
where
    NamespacePublicKey: McNamespacePublicKey + Verifier<NamespaceSignature>,
    UserPublicKey: McPublicUserKey<UserSignature>,
    NamespaceSignature: EncodableSync + EncodableKnownSize + Clone,
    UserSignature: EncodableSync + EncodableKnownSize + Clone,
    PD: PayloadDigest + EncodableSync + EncodableKnownSize,
{
    fn is_authorised_write(
        &self,
        entry: &Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
    ) -> bool {
        match self.capability.access_mode() {
            AccessMode::Read => return false,
            AccessMode::Write => {}
        }

        if !self.capability.granted_area().includes_entry(entry) {
            return false;
        }

        let message = entry.sync_encode_into_boxed_slice();

        if self
            .capability
            .receiver()
            .verify(&message, &self.signature)
            .is_err()
        {
            return false;
        }

        true
    }
}

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;

#[cfg(feature = "dev")]
impl<'a, const MCL: usize, const MCC: usize, const MPL: usize> Arbitrary<'a>
    for McAuthorisationToken<MCL, MCC, MPL, SillyPublicKey, SillySig, SillyPublicKey, SillySig>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let capability: McCapability<
            MCL,
            MCC,
            MPL,
            SillyPublicKey,
            SillySig,
            SillyPublicKey,
            SillySig,
        > = Arbitrary::arbitrary(u)?;

        let signature: SillySig = Arbitrary::arbitrary(u)?;

        Ok(Self {
            capability,
            signature,
        })
    }
}
