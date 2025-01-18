use signature::Verifier;
use ufotofu_codec::{DecodableCanonic, EncodableKnownSize, EncodableSync};
use willow_data_model::{NamespaceId, SubspaceId};

use crate::IsCommunal;

/// An extension of [`NamespaceId`] augmented with traits required by Meadowcap.
pub trait McNamespaceId:
    NamespaceId + EncodableSync + EncodableKnownSize + DecodableCanonic + IsCommunal
{
}

/// An extension of [`SubspaceId`] augmented with traits required by Meadowcap.
pub trait McSubspaceId<UserSignature>:
    SubspaceId + EncodableSync + EncodableKnownSize + DecodableCanonic + Verifier<UserSignature>
{
}
