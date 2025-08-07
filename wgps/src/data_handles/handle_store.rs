use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;

/// Returned when a `HandleStore` could not free the data associated with a given handle.
pub enum FreeHandleError<E> {
    HandleNotBound,
    StoreError(E),
}

/// Returned when a `HandleStore` could not confirm a handle as being freed.
/// Returned when a `HandleStore` could not free the data associated with a given handle.
pub enum ConfirmFreeHandleError<E> {
    HandleNotBound,
    HandleNotMarkedForFreeing,
    StoreError(E),
}

/// A store mapping numeric handles to data of type `DataType`.
pub trait HandleStore<DataType> {
    type StoreError: Error;

    /// Returns the data bound to the given handle (possibly None), or an error.
    fn get(&self, handle: u64) -> Result<Option<&DataType>, Self::StoreError>;

    /// Binds some data to a handle and returns the corresponding handle, or fails in the case of an internal store error.
    fn bind(&mut self, data: DataType) -> Result<u64, Self::StoreError>;

    /// Mark the handle for eventually freeing, that is, to remove the binding between the handle and its corresponding data.
    fn mark_for_freeing(
        &mut self,
        handle: u64,
        me: bool,
    ) -> Result<(), FreeHandleError<Self::StoreError>>;

    /// Mark the handle for eventually freeing, that is, to remove the binding between the handle and its corresponding data.
    fn confirm_freed(
        &mut self,
        handle: u64,
    ) -> Result<(), ConfirmFreeHandleError<Self::StoreError>>;
}

/// A [`HandleStore`] implemented over an ordinary [`HashMap`].
#[derive(Debug, Clone)]
pub struct HashMapHandleStore<DataType> {
    /// Mapping of handle to a tuple of (data, marked for freeing).
    data: HashMap<u64, (DataType, DataHandleState)>,
    least_unassigned_handle: u64,
    handle_limit: u64,
}

impl<DataType> HashMapHandleStore<DataType> {
    pub fn new(handle_limit: u64) -> Self {
        HashMapHandleStore {
            data: HashMap::new(),
            least_unassigned_handle: 0,
            handle_limit,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HashMapHandleStoreError {
    HandleLimitReached,
}

impl Display for HashMapHandleStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashMapHandleStoreError::HandleLimitReached => {
                write!(f, "HashMapHandleStore reached its handle limit",)
            }
        }
    }
}

impl Error for HashMapHandleStoreError {}

impl<DataType> HandleStore<DataType> for HashMapHandleStore<DataType> {
    type StoreError = HashMapHandleStoreError;

    fn get(&self, handle: u64) -> Result<Option<&DataType>, Self::StoreError> {
        match self.data.get(&handle) {
            Some((data, _state)) => Ok(Some(data)),
            None => Ok(None),
        }
    }

    fn bind(&mut self, data: DataType) -> Result<u64, Self::StoreError> {
        let handle = self.least_unassigned_handle;

        self.data
            .insert(handle, (data, DataHandleState::FullyBound));

        self.least_unassigned_handle += 1;

        if self.least_unassigned_handle > self.handle_limit {
            Err(HashMapHandleStoreError::HandleLimitReached)
        } else {
            Ok(handle)
        }
    }

    fn mark_for_freeing(
        &mut self,
        handle: u64,
        me: bool,
    ) -> Result<(), FreeHandleError<Self::StoreError>> {
        match self.data.get_mut(&handle) {
            Some((_data, state)) => match state {
                DataHandleState::FullyBound => {
                    if me {
                        *state = DataHandleState::MeFreed
                    } else {
                        *state = DataHandleState::YouFreed
                    }

                    Ok(())
                }
                DataHandleState::MeFreed => {
                    if me {
                        Ok(())
                    } else {
                        *state = DataHandleState::Waiting;
                        Ok(())
                    }
                }
                DataHandleState::YouFreed => {
                    if me {
                        *state = DataHandleState::Waiting;
                        Ok(())
                    } else {
                        Ok(())
                    }
                }
                DataHandleState::Waiting => Ok(()),
            },
            None => Err(FreeHandleError::HandleNotBound),
        }
    }

    fn confirm_freed(
        &mut self,
        handle: u64,
    ) -> Result<(), ConfirmFreeHandleError<Self::StoreError>> {
        match self.data.get(&handle) {
            Some((_data, DataHandleState::FullyBound)) => {
                self.data.remove(&handle);

                Ok(())
            }
            Some((_data, _)) => Err(ConfirmFreeHandleError::HandleNotMarkedForFreeing),
            None => Err(ConfirmFreeHandleError::HandleNotBound),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DataHandleState {
    FullyBound,
    MeFreed,
    YouFreed,
    Waiting,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn bind_increment_get() {
        let mut store = HashMapHandleStore::<char>::new(0);

        let res_1 = store.get(0);

        assert_eq!(res_1, Ok(None));

        let handle_0 = store.bind('a');

        assert_eq!(handle_0, Ok(0));

        let res_2 = store.get(handle_0.unwrap());

        assert_eq!(res_2, Ok(Some(&'a')));

        let handle_1 = store.bind('b');

        assert_eq!(handle_1, Ok(1));

        let res_3 = store.get(handle_1.unwrap());

        assert_eq!(res_3, Ok(Some(&'b')));
    }
}
