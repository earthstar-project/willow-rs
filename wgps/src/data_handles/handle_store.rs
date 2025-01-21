use std::{error::Error, fmt::Display};

/// Returned when a `HandleStore` could not retrieve some data by a given handle.
pub enum GetHandleError<E> {
    HandleNotBound,
    HandleFreed,
    StoreError(E),
}

/// Returned when a `HandleStore` could not bind some data to a handle.
pub enum BindError<E> {
    LimitReached,
    StoreError(E),
}

/// Returned when a `HandleStore` could not update some data bound to a given handle.
pub enum UpdateHandleError<E> {
    HandleNotBound,
    HandleFreed,
    StoreError(E),
}

/// Returned when a `HandleStore` could not free the data associated with a given handle.
pub enum FreeHandleError<E> {
    HandleNotBound,
    StoreError(E),
}

/// A store mapping numeric handles to data of type `DataType`.
pub trait HandleStore<DataType> {
    const LIMIT: u64;

    type GetError: Display + Error;
    type BindError: Display + Error;
    type FreeError: Display + Error;

    /// Returns the data bound to the given handle, or an error if no data has not been bound, freed, or if the handle store experiences an internal error.
    async fn get(handle: u64) -> Result<DataType, GetHandleError<Self::GetError>>;

    // We needed this in the js version, though I'm not sure if we need it anymore.
    /// *Eventually* returns the data to be bound to some handle, even if that data hasn't been received yet.
    async fn get_eventually(handle: u64) -> Result<DataType, Self::GetError>;

    /// Binds some data to a handle and returns the corresponding handle, or fails in the case of an internal store error.
    async fn bind(data: DataType) -> Result<u64, BindError<Self::BindError>>;

    // We needed this in the JS version to update data associated with PAI (iirc whether some fragment had been replied to)
    // As we're modifying PAI, maybe we don't need this anymore?
    /// Update the data bound to some handle.
    async fn update(handle: u64, data: DataType) -> Result<(), UpdateHandleError<Self::BindError>>;

    /// Mark the handle for eventually freeing, that is, to remove the binding between the handle and its corresponding data.
    async fn mark_for_freeing(handle: u64) -> Result<(), FreeHandleError<Self::FreeError>>;

    /// Returns whether the given handle has had any data bound to it.
    async fn is_handle_bound(handle: u64) -> bool;
}
