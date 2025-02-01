use core::{
    cell::UnsafeCell,
    fmt, mem,
    ops::{Deref, DerefMut},
};

/// A cell that unsafely grants mutable access to its wrapped value. In test builds (`#[cfg(test)]`), it performs runtime checks to panic when mutable access is not exclusive. In non-test builds, it has no runtime overhead and silently allows for undefined behaviour.
#[derive(Debug)]
pub struct FairlyUnsafeCell<T: ?Sized>(UnsafeCell<T>);

impl<T> FairlyUnsafeCell<T> {
    /// Creates a new `FairlyUnsafeCell` containing `value`.
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(5);
    /// ```
    #[inline]
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// Consumes the `FairlyUnsafeCell`, returning the wrapped value.
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(5);
    /// assert_eq!(c.into_inner(), 5);
    /// ```
    #[inline]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    /// Replaces the wrapped value with a new one, returning the old value,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`core::mem::replace`].
    ///
    /// # Safety
    ///
    /// UB if the value is currently borrowed. Will panic instead of causing UB if `#[cfg(test)]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let cell = FairlyUnsafeCell::new(5);
    /// let old_value = unsafe { cell.replace(6) };
    /// assert_eq!(old_value, 5);
    /// assert_eq!(cell.into_inner(), 6);
    /// ```
    #[inline]
    pub unsafe fn replace(&self, t: T) -> T {
        mem::replace(&mut *self.borrow_mut(), t)
    }

    /// Replaces the wrapped value with a new one computed from `f`, returning
    /// the old value, without deinitializing either one.
    ///
    /// # Safety
    ///
    /// UB if the value is currently borrowed. Will panic instead of causing UB if `#[cfg(test)]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let cell = FairlyUnsafeCell::new(5);
    /// let old_value = unsafe { cell.replace_with(|&mut old| old + 1) };
    /// assert_eq!(old_value, 5);
    /// assert_eq!(cell.into_inner(), 6);
    /// ```
    #[inline]
    pub unsafe fn replace_with<F: FnOnce(&mut T) -> T>(&self, f: F) -> T {
        let mut_borrow = &mut *self.borrow_mut();
        let replacement = f(mut_borrow);
        mem::replace(mut_borrow, replacement)
    }

    /// Swaps the wrapped value of `self` with the wrapped value of `other`,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`core::mem::swap`].
    ///
    /// # Safety
    ///
    /// UB if the value in either `FairlyUnsafeCell` is currently borrowed, or
    /// if `self` and `other` point to the same `FairlyUnsafeCell`.
    /// Will panic instead of causing UB if `#[cfg(test)]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(5);
    /// let d = FairlyUnsafeCell::new(6);
    /// unsafe { c.swap(&d) };
    /// assert_eq!(c.into_inner(), 6);
    /// assert_eq!(d.into_inner(), 5);
    /// ```
    #[inline]
    pub unsafe fn swap(&self, other: &Self) {
        mem::swap(&mut *self.borrow_mut(), &mut *other.borrow_mut())
    }
}

impl<T: ?Sized> FairlyUnsafeCell<T> {
    /// Immutably borrows the wrapped value.
    ///
    /// The borrow lasts until the returned `Ref` exits scope. Multiple
    /// immutable borrows can be taken out at the same time.
    ///
    /// # Safety
    ///
    /// UB if the value is currently mutably borrowed.
    /// Will panic instead of causing UB if `#[cfg(test)]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(5);
    ///
    /// unsafe {
    ///     let borrowed_five = c.borrow();
    ///     let borrowed_five2 = c.borrow();
    /// }
    /// ```
    #[inline]
    pub unsafe fn borrow(&self) -> Ref<'_, T> {
        Ref(unsafe { &*self.0.get() })
    }

    /// Mutably borrows the wrapped value.
    ///
    /// The borrow lasts until the returned `RefMut` or all `RefMut`s derived
    /// from it exit scope. The value cannot be borrowed while this borrow is
    /// active.
    ///
    /// # Safety
    ///
    /// UB if the value is currently borrowed.
    /// Will panic instead of causing UB if `#[cfg(test)]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new("hello".to_owned());
    ///
    /// unsafe {
    ///     *c.borrow_mut() = "bonjour".to_owned();
    /// }
    ///
    /// assert_eq!(unsafe { &*c.borrow() }, "bonjour");
    /// ```
    #[inline]
    pub unsafe fn borrow_mut(&self) -> RefMut<'_, T> {
        RefMut(unsafe { &mut *self.0.get() })
    }

    /// Returns a raw pointer to the underlying data in this cell.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(5);
    ///
    /// let ptr = c.as_ptr();
    /// ```
    #[inline]
    pub fn as_ptr(&self) -> *mut T {
        self.0.get()
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows the `UnsafeCell` mutably (at compile-time) which
    /// guarantees that we possess the only reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let mut c = FairlyUnsafeCell::new(5);
    /// *c.get_mut() += 1;
    ///
    /// assert_eq!(unsafe { *c.borrow() }, 6);
    /// ```
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}

impl<T: Default> Default for FairlyUnsafeCell<T> {
    /// Creates a `FairlyUnsafeCell<T>`, with the `Default` value for T.
    #[inline]
    fn default() -> FairlyUnsafeCell<T> {
        Self(UnsafeCell::default())
    }
}

impl<T> From<T> for FairlyUnsafeCell<T> {
    /// Creates a new `FairlyUnsafeCell<T>` containing the given value.
    fn from(t: T) -> FairlyUnsafeCell<T> {
        Self(UnsafeCell::from(t))
    }
}

/// A wrapper type for an immutably borrowed value from a `FairlyUnsafeCell<T>`.
///
/// See the [module-level documentation](crate) for more.
#[derive(Debug)]
pub struct Ref<'b, T>(&'b T)
where
    T: 'b + ?Sized;

impl<'b, T: ?Sized> Ref<'b, T> {
    /// Copies a `Ref`.
    ///
    /// The `FairlyUnsafeCell` is already immutably borrowed, so this cannot introduce UB where there was none before.
    ///
    /// This is an associated function that needs to be used as
    /// `Ref::clone(...)`. A `Clone` implementation or a method would interfere
    /// with the widespread use of `r.borrow().clone()` to clone the contents of
    /// a `FairlyUnsafeCell`.
    #[must_use]
    #[inline]
    pub fn clone(orig: &Ref<'b, T>) -> Ref<'b, T> {
        Ref(orig.0)
    }

    /// Makes a new `Ref` for a component of the borrowed data.
    ///
    /// The `FairlyUnsafeCell` is already immutably borrowed, so this cannot introduce UB where there was none before.
    ///
    /// This is an associated function that needs to be used as `Ref::map(...)`.
    /// A method would interfere with methods of the same name on the contents
    /// of a `FairlyUnsafeCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new((5, 'b'));
    /// let b1: Ref<'_, (u32, char)> = unsafe { c.borrow() };
    /// let b2: Ref<'_, u32> = Ref::map(b1, |t| &t.0);
    /// assert_eq!(*b2, 5)
    /// ```
    #[inline]
    pub fn map<U: ?Sized, F>(orig: Ref<'b, T>, f: F) -> Ref<'b, U>
    where
        F: FnOnce(&T) -> &U,
    {
        Ref(f(orig.0))
    }

    /// Makes a new `Ref` for an optional component of the borrowed data. The
    /// original guard is returned as an `Err(..)` if the closure returns
    /// `None`.
    ///
    /// The `FairlyUnsafeCell` is already immutably borrowed, so this cannot introduce UB where there was none before.
    ///
    /// This is an associated function that needs to be used as
    /// `Ref::filter_map(...)`. A method would interfere with methods of the same
    /// name on the contents of a `FairlyUnsafeCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new(vec![1, 2, 3]);
    /// let b1: Ref<'_, Vec<u32>> = unsafe { c.borrow() };
    /// let b2: Result<Ref<'_, u32>, _> = Ref::filter_map(b1, |v| v.get(1));
    /// assert_eq!(*b2.unwrap(), 2);
    /// ```
    #[inline]
    pub fn filter_map<U: ?Sized, F>(orig: Ref<'b, T>, f: F) -> Result<Ref<'b, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        match f(orig.0) {
            Some(yay) => Ok(Ref(yay)),
            None => Err(orig),
        }
    }

    /// Splits a `Ref` into multiple `Ref`s for different components of the
    /// borrowed data.
    ///
    /// The `FairlyUnsafeCell` is already immutably borrowed, so this cannot introduce UB where there was none before.
    ///
    /// This is an associated function that needs to be used as
    /// `Ref::map_split(...)`. A method would interfere with methods of the same
    /// name on the contents of a `FairlyUnsafeCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let cell = FairlyUnsafeCell::new([1, 2, 3, 4]);
    /// let borrow = unsafe { cell.borrow() };
    /// let (begin, end) = Ref::map_split(borrow, |slice| slice.split_at(2));
    /// assert_eq!(*begin, [1, 2]);
    /// assert_eq!(*end, [3, 4]);
    /// ```
    #[inline]
    pub fn map_split<U: ?Sized, V: ?Sized, F>(orig: Ref<'b, T>, f: F) -> (Ref<'b, U>, Ref<'b, V>)
    where
        F: FnOnce(&T) -> (&U, &V),
    {
        let (a, b) = f(orig.0);
        (Ref(a), Ref(b))
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Ref<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A wrapper type for a mutably borrowed value from a `FairlyUnsafeCell<T>`.
///
/// See the [module-level documentation](crate) for more.
#[derive(Debug)]
pub struct RefMut<'b, T>(&'b mut T)
where
    T: 'b + ?Sized;

impl<'b, T: ?Sized> RefMut<'b, T> {
    /// Makes a new `RefMut` for a component of the borrowed data, e.g., an enum
    /// variant.
    ///
    /// The `FairlyUnsafeCell` is already mutably borrowed, so this cannot fail.
    ///
    /// This is an associated function that needs to be used as
    /// `RefMut::map(...)`. A method would interfere with methods of the same
    /// name on the contents of a `FairlyUnsafeCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let c = FairlyUnsafeCell::new((5, 'b'));
    /// {
    ///     let b1: RefMut<'_, (u32, char)> = unsafe { c.borrow_mut() };
    ///     let mut b2: RefMut<'_, u32> = RefMut::map(b1, |t| &mut t.0);
    ///     assert_eq!(*b2, 5);
    ///     *b2 = 42;
    /// }
    /// assert_eq!(unsafe { *c.borrow() }, (42, 'b'));
    /// ```
    #[inline]
    pub fn map<U: ?Sized, F>(orig: RefMut<'b, T>, f: F) -> RefMut<'b, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        RefMut(f(orig.0))
    }

    // /// Makes a new `RefMut` for an optional component of the borrowed data. The
    // /// original guard is returned as an `Err(..)` if the closure returns
    // /// `None`.
    // ///
    // /// The `FairlyUnsafeCell` is already mutably borrowed, so this cannot fail.
    // ///
    // /// This is an associated function that needs to be used as
    // /// `RefMut::filter_map(...)`. A method would interfere with methods of the
    // /// same name on the contents of a `FairlyUnsafeCell` used through `Deref`.
    // ///
    // /// # Examples
    // ///
    // /// ```
    // /// use fairly_unsafe_cell::*;
    // ///
    // /// let c = FairlyUnsafeCell::new(vec![1, 2, 3]);
    // ///
    // /// {
    // ///     let b1: RefMut<'_, Vec<u32>> = unsafe { c.borrow_mut() };
    // ///     let mut b2: Result<RefMut<'_, u32>, _> = RefMut::filter_map(b1, |v| v.get_mut(1));
    // ///
    // ///     if let Ok(mut b2) = b2 {
    // ///         *b2 += 2;
    // ///     }
    // /// }
    // ///
    // /// assert_eq!(* unsafe { c.borrow() }, vec![1, 4, 3]);
    // /// ```
    // #[inline]
    // pub fn filter_map<U: ?Sized, F>(orig: RefMut<'b, T>, f: F) -> Result<RefMut<'b, U>, Self>
    // where
    //     F: FnOnce(&mut T) -> Option<&mut U>,
    // {
    //    { if let Some(yay) = f(&mut  *orig.0) {
    //         return Ok(RefMut(yay));
    //     }}

    //     Err(RefMut(orig.0))
    // }

    /// Splits a `RefMut` into multiple `RefMut`s for different components of the
    /// borrowed data.
    ///
    /// The underlying `FairlyUnsafeCell` will remain mutably borrowed until both
    /// returned `RefMut`s go out of scope.
    ///
    /// The `FairlyUnsafeCell` is already mutably borrowed, so this cannot fail.
    ///
    /// This is an associated function that needs to be used as
    /// `RefMut::map_split(...)`. A method would interfere with methods of the
    /// same name on the contents of a `FairlyUnsafeCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fairly_unsafe_cell::*;
    ///
    /// let cell = FairlyUnsafeCell::new([1, 2, 3, 4]);
    /// let borrow = unsafe { cell.borrow_mut() };
    /// let (mut begin, mut end) = RefMut::map_split(borrow, |slice| slice.split_at_mut(2));
    /// assert_eq!(*begin, [1, 2]);
    /// assert_eq!(*end, [3, 4]);
    /// begin.copy_from_slice(&[4, 3]);
    /// end.copy_from_slice(&[2, 1]);
    /// ```
    #[inline]
    pub fn map_split<U: ?Sized, V: ?Sized, F>(
        orig: RefMut<'b, T>,
        f: F,
    ) -> (RefMut<'b, U>, RefMut<'b, V>)
    where
        F: FnOnce(&mut T) -> (&mut U, &mut V),
    {
        let (a, b) = f(orig.0);
        (RefMut(a), RefMut(b))
    }
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for RefMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
