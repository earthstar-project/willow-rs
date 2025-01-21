use std::{error::Error, fmt::Display};

/// Returned when a `HandleStore` could not free the data associated with a given handle.
pub enum FreeHandleError<E> {
    HandleNotBound,
    StoreError(E),
}

/// A store mapping numeric handles to data of type `DataType`.
pub trait HandleStore<DataType> {
    type GetError: Display + Error;
    type BindError: Display + Error;
    type FreeError: Display + Error;

    /// Returns the data bound to the given handle, or an error if no data has not been bound, freed, or if the handle store experiences an internal error.
    fn get(handle: u64) -> Result<Option<DataType>, Self::GetError>;

    /// Binds some data to a handle and returns the corresponding handle, or fails in the case of an internal store error.
    fn bind(data: DataType) -> Result<u64, Self::BindError>;

    /// Mark the handle for eventually freeing, that is, to remove the binding between the handle and its corresponding data.
    fn mark_for_freeing(handle: u64) -> Result<(), FreeHandleError<Self::FreeError>>;

    /// Returns whether the given handle has had any data bound to it.
    fn is_handle_bound(handle: u64) -> bool;
}
