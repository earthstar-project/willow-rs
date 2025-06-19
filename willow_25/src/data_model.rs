use crate::{AuthorisationToken, NamespaceId25, PayloadDigest25, SubspaceId25};

pub type Component<'a> = willow_data_model::Component<'a, 1024>;
pub type OwnedComponent = willow_data_model::OwnedComponent<1024>;
pub type PathBuilder = willow_data_model::PathBuilder<1024, 1024, 1024>;
pub type Path = willow_data_model::Path<1024, 1024, 1024>;
pub type Entry =
    willow_data_model::Entry<1024, 1024, 1024, NamespaceId25, SubspaceId25, PayloadDigest25>;
pub type LengthyEntry =
    willow_data_model::LengthyEntry<1024, 1024, 1024, NamespaceId25, SubspaceId25, PayloadDigest25>;
pub type AuthorisedEntry = willow_data_model::AuthorisedEntry<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
>;
pub type LengthyAuthorisedEntry = willow_data_model::LengthyAuthorisedEntry<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
>;

// Grouping

pub type Area = willow_data_model::grouping::Area<1024, 1024, 1024, SubspaceId25>;
pub type Range3d = willow_data_model::grouping::Range3d<1024, 1024, 1024, SubspaceId25>;
pub type AreaOfInterest =
    willow_data_model::grouping::AreaOfInterest<1024, 1024, 1024, SubspaceId25>;

// Private areas

pub type PrivateAreaContext =
    willow_data_model::PrivateAreaContext<1024, 1024, 1024, NamespaceId25, SubspaceId25>;
pub type PrivateInterest =
    willow_data_model::PrivateInterest<1024, 1024, 1024, NamespaceId25, SubspaceId25>;
pub type PrivatePathContext = willow_data_model::PrivatePathContext<1024, 1024, 1024>;

// Store

pub type EventSystem<Err> = willow_data_model::EventSystem<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
    Err,
>;

pub type Subscriber<Err> = willow_data_model::Subscriber<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
    Err,
>;

pub type EntryIngestionSuccess = willow_data_model::EntryIngestionSuccess<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
>;

pub type StoreEvent = willow_data_model::StoreEvent<
    1024,
    1024,
    1024,
    NamespaceId25,
    SubspaceId25,
    PayloadDigest25,
    AuthorisationToken,
>;

// Straight re-exports for convenience' sake.
pub use willow_data_model::grouping::AreaSubspace;
pub use willow_data_model::grouping::Range;
pub use willow_data_model::AreaNotAlmostIncludedError;
pub use willow_data_model::ComponentsNotRelatedError;
pub use willow_data_model::EntryIngestionError;
pub use willow_data_model::EntryOrigin;
pub use willow_data_model::ForgetEntryError;
pub use willow_data_model::ForgetPayloadError;
pub use willow_data_model::InternalSubscriber;
pub use willow_data_model::InvalidPathError;
pub use willow_data_model::PayloadAppendError;
pub use willow_data_model::PayloadError;
pub use willow_data_model::QueryIgnoreParams;
pub use willow_data_model::Store;
pub use willow_data_model::Timestamp;
pub use willow_data_model::UnauthorisedWriteError;
