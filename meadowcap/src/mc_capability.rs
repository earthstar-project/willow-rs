#[cfg(feature = "dev")]
use arbitrary::Arbitrary;
use either::Either;
use signature::{Error as SignatureError, Signer, Verifier};
use ufotofu::sync::consumer::IntoVec;
use willow_data_model::{grouping::Area, Entry, NamespaceId, PayloadDigest, SubspaceId};
use willow_encoding::sync::Encodable;

use crate::{
    communal_capability::{CommunalCapability, NamespaceIsNotCommunalError},
    mc_authorisation_token::McAuthorisationToken,
    owned_capability::{OwnedCapability, OwnedCapabilityCreationError},
    AccessMode, Delegation, FailedDelegationError, InvalidDelegationError, IsCommunal,
};

/// Returned when an operation only applicable to a capability with access mode [`AccessMode::Write`] was called on a capability with access mode [`AccessMode::Read`].
#[derive(Debug)]
pub struct NotAWriteCapabilityError;

impl core::fmt::Display for NotAWriteCapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tried to perform an operation on a read capability which is only permitted on write capabilities."
        )
    }
}

impl std::error::Error for NotAWriteCapabilityError {}

/// A Meadowcap capability.
///
/// [Definition](https://willowprotocol.org/specs/meadowcap/index.html#Capability)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "dev", derive(Arbitrary))]
pub enum McCapability<
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
    Communal(CommunalCapability<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, UserSignature>),
    Owned(
        OwnedCapability<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
    ),
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
    McCapability<
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
{
    /// Creates a new communal capability granting access to the [`SubspaceId`] corresponding to the given `UserPublicKey`, or return an error if the namespace key is not communal.
    pub fn new_communal(
        namespace_key: NamespacePublicKey,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, NamespaceIsNotCommunalError<NamespacePublicKey>> {
        let cap = CommunalCapability::new(namespace_key, user_key, access_mode)?;
        Ok(Self::Communal(cap))
    }

    /// Creates a new owned capability granting access to the [full area](https://willowprotocol.org/specs/grouping-entries/index.html#full_area) of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) to the given `UserPublicKey`.
    pub fn new_owned<NamespaceSecret>(
        namespace_key: NamespacePublicKey,
        namespace_secret: &NamespaceSecret,
        user_key: UserPublicKey,
        access_mode: AccessMode,
    ) -> Result<Self, OwnedCapabilityCreationError<NamespacePublicKey>>
    where
        NamespaceSecret: Signer<NamespaceSignature>,
    {
        let cap = OwnedCapability::new(namespace_key, namespace_secret, user_key, access_mode)?;

        Ok(Self::Owned(cap))
    }

    /// Returns the kind of access this capability grants.
    pub fn access_mode(&self) -> AccessMode {
        match self {
            Self::Communal(cap) => cap.access_mode(),
            Self::Owned(cap) => cap.access_mode(),
        }
    }

    /// Returns the public key of the user to whom this capability grants access.
    pub fn receiver(&self) -> &UserPublicKey {
        match self {
            Self::Communal(cap) => cap.receiver(),
            Self::Owned(cap) => cap.receiver(),
        }
    }

    /// Returns the public key of the [namespace](https://willowprotocol.org/specs/data-model/index.html#namespace) for which this capability grants access.
    pub fn granted_namespace(&self) -> &NamespacePublicKey {
        match self {
            Self::Communal(cap) => cap.granted_namespace(),
            Self::Owned(cap) => cap.granted_namespace(),
        }
    }

    /// Returns the [`Area`] for which this capability grants access.
    pub fn granted_area(&self) -> Area<MCL, MCC, MPL, UserPublicKey> {
        match self {
            Self::Communal(cap) => cap.granted_area(),
            Self::Owned(cap) => cap.granted_area(),
        }
    }

    /// Returns a slice of all [`Delegation`]s made to this capability.
    pub fn delegations(
        &self,
    ) -> impl ExactSizeIterator<Item = &Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>>
    {
        match self {
            McCapability::Communal(cap) => cap.delegations_(),
            McCapability::Owned(cap) => cap.delegations_(),
        }
    }

    /// Returns the number of delegations present on this capability.
    pub fn delegations_len(&self) -> usize {
        match self {
            McCapability::Communal(cap) => cap.delegations_len(),
            McCapability::Owned(cap) => cap.delegations_len(),
        }
    }

    /// Returns the public key of the very first user this capability was issued to.
    pub fn progenitor(&self) -> &UserPublicKey {
        match self {
            McCapability::Communal(cap) => cap.progenitor(),
            McCapability::Owned(cap) => cap.progenitor(),
        }
    }

    /// Delegates this capability to a new `UserPublicKey` for a given [`willow_data_model::grouping::Area`].
    /// Will fail if the area is not included by this capability's granted area, or if the given secret key does not correspond to the capability's receiver.
    pub fn delegate<UserSecretKey>(
        &self,
        secret_key: &UserSecretKey,
        new_user: &UserPublicKey,
        new_area: &Area<MCL, MCC, MPL, UserPublicKey>,
    ) -> Result<Self, FailedDelegationError<MCL, MCC, MPL, UserPublicKey>>
    where
        UserSecretKey: Signer<UserSignature>,
    {
        let delegated = match self {
            McCapability::Communal(cap) => {
                let delegated = cap.delegate(secret_key, new_user, new_area)?;

                Self::Communal(delegated)
            }
            McCapability::Owned(cap) => {
                let delegated = cap.delegate(secret_key, new_user, new_area)?;

                Self::Owned(delegated)
            }
        };

        Ok(delegated)
    }

    /// Appends an existing delegation to an existing capability, or return an error if the delegation is invalid.
    pub fn append_existing_delegation(
        &mut self,
        delegation: Delegation<MCL, MCC, MPL, UserPublicKey, UserSignature>,
    ) -> Result<(), InvalidDelegationError<MCL, MCC, MPL, UserPublicKey, UserSignature>> {
        match self {
            McCapability::Communal(cap) => cap.append_existing_delegation(delegation),
            McCapability::Owned(cap) => cap.append_existing_delegation(delegation),
        }
    }

    /// Returns a new AuthorisationToken without checking if the resulting signature is correct (e.g. because you are going to immediately do that by constructing an [`willow_data_model::AuthorisedEntry`]).
    ///
    /// ## Safety
    ///
    /// This function must be called with this capability's [receiver](https://willowprotocol.org/specs/meadowcap/index.html#communal_cap_receiver)'s corresponding secret key, or a token with an incorrect signature will be produced.
    pub unsafe fn authorisation_token_unchecked<UserSecret, PD>(
        &self,
        entry: Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        secret: UserSecret,
    ) -> Result<
        McAuthorisationToken<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        NotAWriteCapabilityError,
    >
    where
        UserSecret: Signer<UserSignature>,
        PD: PayloadDigest + Encodable,
    {
        match self.access_mode() {
            AccessMode::Read => Err(NotAWriteCapabilityError),
            AccessMode::Write => {
                let mut consumer = IntoVec::<u8>::new();
                entry.encode(&mut consumer).unwrap();

                let signature = secret.sign(&consumer.into_vec());

                Ok(McAuthorisationToken {
                    capability: self.clone(),
                    signature,
                })
            }
        }
    }

    /// Returns a new [`McAuthorisationToken`], or an error if the resulting signature was not for the capability's receiver.
    pub fn authorisation_token<UserSecret, PD>(
        &self,
        entry: &Entry<MCL, MCC, MPL, NamespacePublicKey, UserPublicKey, PD>,
        secret: UserSecret,
    ) -> Result<
        McAuthorisationToken<
            MCL,
            MCC,
            MPL,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        >,
        Either<NotAWriteCapabilityError, SignatureError>,
    >
    where
        UserSecret: Signer<UserSignature>,
        PD: PayloadDigest + Encodable,
    {
        match self.access_mode() {
            AccessMode::Read => Err(Either::Left(NotAWriteCapabilityError)),
            AccessMode::Write => {
                let mut consumer = IntoVec::<u8>::new();
                entry.encode(&mut consumer).unwrap();

                let message = consumer.into_vec();

                let signature = secret.sign(&message);

                self.receiver()
                    .verify(&message, &signature)
                    .map_err(Either::Right)?;

                Ok(McAuthorisationToken {
                    capability: self.clone(),
                    signature,
                })
            }
        }
    }
}

use syncify::syncify;
use syncify::syncify_replace;

#[syncify(encoding_sync)]
pub(super) mod encoding {
    use super::*;

    #[syncify_replace(use ufotofu::sync::{BulkConsumer, BulkProducer};)]
    use ufotofu::local_nb::{BulkConsumer, BulkProducer};

    #[syncify_replace(use willow_encoding::sync::{Encodable, Decodable, RelativeDecodable, RelativeEncodable};)]
    use willow_encoding::{Decodable, Encodable, RelativeDecodable, RelativeEncodable};

    use willow_encoding::{
        is_bitflagged, sync::Encodable as EncodableSync, CompactWidth, DecodeError,
    };

    #[syncify_replace(use willow_encoding::sync::produce_byte;)]
    use willow_encoding::produce_byte;

    #[syncify_replace(
        use willow_encoding::sync::{encode_compact_width_be, decode_compact_width_be};
    )]
    use willow_encoding::{decode_compact_width_be, encode_compact_width_be};

    impl<
            const MCL: usize,
            const MCC: usize,
            const MPL: usize,
            NamespacePublicKey,
            NamespaceSignature,
            UserPublicKey,
            UserSignature,
        > RelativeEncodable<Area<MCL, MCC, MPL, UserPublicKey>>
        for McCapability<
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
            NamespaceId + EncodableSync + Encodable + Verifier<NamespaceSignature> + IsCommunal,
        UserPublicKey: SubspaceId + EncodableSync + Encodable + Verifier<UserSignature>,
        NamespaceSignature: EncodableSync + Encodable + Clone,
        UserSignature: EncodableSync + Encodable + Clone,
    {
        async fn relative_encode<Consumer>(
            &self,
            out: &Area<MCL, MCC, MPL, UserPublicKey>,
            consumer: &mut Consumer,
        ) -> Result<(), Consumer::Error>
        where
            Consumer: BulkConsumer<Item = u8>,
        {
            let mut header: u8 = 0;

            match self {
                McCapability::Communal(_) => {
                    if self.access_mode() == AccessMode::Write {
                        header |= 0b0100_0000;
                    }
                }
                McCapability::Owned(_) => {
                    if self.access_mode() == AccessMode::Read {
                        header |= 0b1000_0000;
                    } else {
                        header |= 0b1100_0000;
                    }
                }
            }

            let delegations_count = self.delegations_len() as u64;

            if delegations_count >= 4294967296 {
                header |= 0b0011_1111;
            } else if delegations_count >= 65536 {
                header |= 0b0011_1110;
            } else if delegations_count >= 256 {
                header |= 0b0011_1101;
            } else if delegations_count >= 60 {
                header |= 0b0011_1100;
            } else {
                header |= delegations_count as u8;
            }

            consumer.consume(header).await?;

            Encodable::encode(self.granted_namespace(), consumer).await?;
            Encodable::encode(self.progenitor(), consumer).await?;

            match self {
                McCapability::Communal(_) => {}
                McCapability::Owned(cap) => {
                    Encodable::encode(cap.initial_authorisation(), consumer).await?;
                }
            };

            if delegations_count >= 60 {
                encode_compact_width_be(delegations_count, consumer).await?;
            }

            let mut prev_area = out.clone();

            for delegation in self.delegations() {
                delegation
                    .area
                    .relative_encode(&prev_area, consumer)
                    .await?;
                prev_area = delegation.area.clone();
                Encodable::encode(&delegation.user, consumer).await?;
                Encodable::encode(&delegation.signature, consumer).await?;
            }

            Ok(())
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
        > RelativeDecodable<Area<MCL, MCC, MPL, UserPublicKey>>
        for McCapability<
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
            NamespaceId + EncodableSync + Decodable + Verifier<NamespaceSignature> + IsCommunal,
        UserPublicKey: SubspaceId + EncodableSync + Decodable + Verifier<UserSignature>,
        NamespaceSignature: EncodableSync + Decodable + Clone,
        UserSignature: EncodableSync + Decodable + Clone,
    {
        async fn relative_decode_canonical<Producer>(
            out: &Area<MCL, MCC, MPL, UserPublicKey>,
            producer: &mut Producer,
        ) -> Result<Self, DecodeError<Producer::Error>>
        where
            Producer: BulkProducer<Item = u8>,
            Self: Sized,
        {
            let header = produce_byte(producer).await?;

            let is_owned = is_bitflagged(header, 0);
            let access_mode = if is_bitflagged(header, 1) {
                AccessMode::Write
            } else {
                AccessMode::Read
            };

            let namespace_key = NamespacePublicKey::decode_canonical(producer).await?;
            let user_key = UserPublicKey::decode_canonical(producer).await?;

            let mut base_cap = if is_owned {
                let initial_authorisation = NamespaceSignature::decode_canonical(producer).await?;

                let cap = OwnedCapability::from_existing(
                    namespace_key,
                    user_key,
                    initial_authorisation,
                    access_mode,
                )
                .map_err(|_| DecodeError::InvalidInput)?;

                Self::Owned(cap)
            } else {
                let cap = CommunalCapability::new(namespace_key, user_key, access_mode)
                    .map_err(|_| DecodeError::InvalidInput)?;

                Self::Communal(cap)
            };

            let delegations_to_decode = if header & 0b0011_1111 == 0b0011_1111 {
                decode_compact_width_be(CompactWidth::Eight, producer).await?
            } else if header & 0b0011_1110 == 0b0011_1110 {
                decode_compact_width_be(CompactWidth::Four, producer).await?
            } else if header & 0b0011_1101 == 0b0011_1101 {
                decode_compact_width_be(CompactWidth::Two, producer).await?
            } else if header & 0b0011_1100 == 0b0011_1100 {
                decode_compact_width_be(CompactWidth::One, producer).await?
            } else {
                (header & 0b0011_1111) as u64
            };

            if header & 0b0011_1100 == 0b0011_1100 && delegations_to_decode < 60 {
                // The delegation count should have been encoded directly in the header.
                return Err(DecodeError::InvalidInput);
            }

            let mut prev_area = out.clone();

            for _ in 0..delegations_to_decode {
                let area = Area::<MCL, MCC, MPL, UserPublicKey>::relative_decode_canonical(
                    &prev_area, producer,
                )
                .await?;
                prev_area = area.clone();
                let user = UserPublicKey::decode_canonical(producer).await?;
                let signature = UserSignature::decode_canonical(producer).await?;

                base_cap
                    .append_existing_delegation(Delegation {
                        area,
                        user,
                        signature,
                    })
                    .map_err(|_| DecodeError::InvalidInput)?;
            }

            Ok(base_cap)
        }
    }
}
