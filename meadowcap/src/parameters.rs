use signature::Verifier;
use ufotofu_codec::{Blame, DecodableCanonic, EncodableKnownSize, EncodableSync};
use willow_data_model::{NamespaceId, SubspaceId};

use crate::IsCommunal;

/// An extension of [`NamespaceId`] augmented with traits required by Meadowcap.
pub trait McNamespacePublicKey:
    NamespaceId
    + EncodableSync
    + EncodableKnownSize
    + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>
    + IsCommunal
{
}

/// An extension of [`SubspaceId`] augmented with traits required by Meadowcap.
pub trait McPublicUserKey<UserSignature>:
    SubspaceId
    + EncodableSync
    + EncodableKnownSize
    + DecodableCanonic<ErrorReason = Blame, ErrorCanonic = Blame>
    + Verifier<UserSignature>
{
}
