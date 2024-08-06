use signature::Verifier;
use ufotofu::sync::consumer::IntoVec;
use willow_data_model::{
    encoding::sync::Encodable, Entry, IsAuthorisedWrite, NamespaceId, PayloadDigest, SubspaceId,
};

use crate::{mc_capability::McCapability, AccessMode, IsCommunal};

/// To be used as an AuthorisationToken for Willow.
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
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    NamespaceSignature: Encodable + Clone,
    UserSignature: Encodable + Clone,
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
        PD,
    > IsAuthorisedWrite<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>
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
    NamespacePublicKey: NamespaceId + Encodable + Verifier<NamespaceSignature> + IsCommunal,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature>,
    NamespaceSignature: Encodable + Clone,
    UserSignature: Encodable + Clone,
    PD: PayloadDigest + Encodable,
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

        let mut consumer = IntoVec::<u8>::new();
        entry.encode(&mut consumer).unwrap();

        if self
            .capability
            .receiver()
            .verify(&consumer.into_vec(), &self.signature)
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
impl<
        'a,
        const MCL: usize,
        const MCC: usize,
        const MPL: usize,
        NamespacePublicKey,
        NamespaceSignature,
        UserPublicKey,
        UserSignature,
    > Arbitrary<'a>
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
    NamespacePublicKey:
        NamespaceId + Encodable + IsCommunal + Arbitrary<'a> + Verifier<NamespaceSignature>,
    UserPublicKey: SubspaceId + Encodable + Verifier<UserSignature> + Arbitrary<'a>,
    NamespaceSignature: Encodable + Clone + Arbitrary<'a>,
    UserSignature: Encodable + Clone + Arbitrary<'a>,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let capability: McCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        > = Arbitrary::arbitrary(u)?;

        let signature: UserSignature = Arbitrary::arbitrary(u)?;

        Ok(Self {
            capability,
            signature,
        })
    }
}
