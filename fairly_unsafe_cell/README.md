# Fairly Unsafe Cell

A hybrid between an `UnsafeCell` and a `RefCell`: comes with an unsafe but RefCell-like API that panics in test builds (`#[cfg(test)]`) when mutable access is not exclusive, but has no overhead (and allows for UB) in non-test builds.