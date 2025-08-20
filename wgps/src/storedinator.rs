use std::{cell::RefCell, collections::HashMap, hash::Hash, marker::PhantomData, rc::Rc};

/// Gives access to (and creates if necessary) stores (which only store data from a single namespace) by namespace. You can (and should) reuse one of these for several concurrent sync sessions.
pub(crate) struct Storedinator<Store, StoreCreationFunction, N> {
    fun: StoreCreationFunction,
    stores: RefCell<HashMap<N, Rc<Store>>>,
}

impl<Store, StoreCreationFunction, N> Storedinator<Store, StoreCreationFunction, N> {
    pub fn new(fun: StoreCreationFunction) -> Self {
        Self {
            fun,
            stores: RefCell::new(HashMap::new()),
        }
    }
}

impl<Store, StoreCreationFunction, N, CreationError> Storedinator<Store, StoreCreationFunction, N>
where
    StoreCreationFunction: Fn(&N) -> Result<Store, CreationError>,
    N: Eq + Hash + Clone,
{
    /// Get (and if necessary first create) the store for entries of the given namespace_id.
    pub fn get_store(&self, namespace_id: &N) -> Result<Rc<Store>, CreationError> {
        let mut map_ref = self.stores.borrow_mut();

        match map_ref.get(namespace_id) {
            Some(store) => return Ok(store.clone()),
            None => {
                map_ref.insert(namespace_id.clone(), Rc::new((self.fun)(namespace_id)?));
                return self.get_store(namespace_id);
            }
        }
    }
}
