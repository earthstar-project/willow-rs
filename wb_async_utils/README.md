# Wormblossom Async Utils

A crate of utilities for working with async code, making a few simplifying assumptions:

- nothing is panic safe, we assume panics to abort rather than rewind, and
- everything is `!Send`, futures will be executed on a single-threaded runtime.

The "synchronisation primitives" in this crate are not about synchronisation across threads, but merely to let different parts of programs `.await` access to some data.