use ed25519_dalek::{SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH};
use meadowcap::McPublicUserKey;
use rand::rngs::OsRng;
use signature::Verifier;
use ufotofu::{BulkConsumer, BulkProducer};
use ufotofu_codec::{
    Blame, Decodable, DecodableCanonic, DecodeError, Encodable, EncodableKnownSize, EncodableSync,
};
use willow_data_model::SubspaceId;

#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use willow_sideload::SideloadSubspaceId;

use crate::SigningKey25;

/// An [ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) public key suitable for the Willow Data Model's [`SubspaceId`](https://willowprotocol.org/specs/data-model/index.html#SubspaceId) parameter, and Meadowcap's [`UserPublicKey`](https://willowprotocol.org/specs/meadowcap/index.html#UserPublicKey) parameter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub struct SubspaceId25([u8; PUBLIC_KEY_LENGTH]);

impl SubspaceId25 {
    /// Returns a new [`SubspaceId25`] and its corresponding signing key.
    pub fn new() -> (Self, SigningKey25) {
        let mut csprng = OsRng;
        let signing_key: SigningKey = SigningKey::generate(&mut csprng);

        (
            Self(signing_key.verifying_key().to_bytes()),
            SigningKey25::new(signing_key),
        )
    }
}

impl Verifier<crate::Signature25> for SubspaceId25 {
    fn verify(&self, msg: &[u8], signature: &crate::Signature25) -> Result<(), signature::Error> {
        let verifying_key = VerifyingKey::from_bytes(&self.0).expect("Tried to use a public key which doesn't actually represent a point on curve25519, probably taken from the result of a successor function.");

        verifying_key.verify(msg, signature.inner())
    }
}

impl PartialOrd for SubspaceId25 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SubspaceId25 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl SubspaceId for SubspaceId25 {
    /// Returns the next possible value in the set of all [`SubspaceId`], i.e. the (unique) least value that is strictly greater than `self`. Only if there is no greater value at all may this method return `None`.
    ///
    /// Returned values may not represent a point on curve25519 and may panic if you try to use `verify` on them. Successors are generally only used to generate upper bounds for [`willow_data_model::grouping::Range`]s.
    fn successor(&self) -> Option<Self> {
        let mut bytes = self.0;

        let can_increment = !bytes.iter().all(|byte| *byte == 255);

        if can_increment {
            for byte_ref in bytes.iter_mut().rev() {
                if *byte_ref == 255 {
                    *byte_ref = 0;
                } else {
                    *byte_ref += 1;
                }
            }

            return Some(Self(bytes));
        }

        None
    }
}

impl McPublicUserKey<crate::Signature25> for SubspaceId25 {}

impl SideloadSubspaceId for SubspaceId25 {
    fn default_subspace_id() -> Self {
        Self(crate::DEFAULT_PUBLIC_KEY)
    }
}

impl Encodable for SubspaceId25 {
    async fn encode<C>(&self, consumer: &mut C) -> Result<(), C::Error>
    where
        C: BulkConsumer<Item = u8>,
    {
        consumer
            .bulk_consume_full_slice(&self.0)
            .await
            .map_err(|err| err.into_reason())?;

        Ok(())
    }
}

impl EncodableSync for SubspaceId25 {}

impl EncodableKnownSize for SubspaceId25 {
    fn len_of_encoding(&self) -> usize {
        PUBLIC_KEY_LENGTH
    }
}

impl Decodable for SubspaceId25 {
    type ErrorReason = Blame;

    async fn decode<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorReason>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice: [u8; PUBLIC_KEY_LENGTH] = [0; PUBLIC_KEY_LENGTH];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        match VerifyingKey::from_bytes(&slice) {
            Ok(_) => Ok(Self(slice)),
            Err(_) => Err(DecodeError::Other(Blame::TheirFault)),
        }
    }
}

impl DecodableCanonic for SubspaceId25 {
    type ErrorCanonic = Blame;

    async fn decode_canonic<P>(
        producer: &mut P,
    ) -> Result<Self, DecodeError<P::Final, P::Error, Self::ErrorCanonic>>
    where
        P: BulkProducer<Item = u8>,
        Self: Sized,
    {
        let mut slice: [u8; PUBLIC_KEY_LENGTH] = [0; PUBLIC_KEY_LENGTH];

        producer.bulk_overwrite_full_slice(&mut slice).await?;

        match VerifyingKey::from_bytes(&slice) {
            Ok(_) => Ok(Self(slice)),
            Err(_) => Err(DecodeError::Other(Blame::TheirFault)),
        }
    }
}
