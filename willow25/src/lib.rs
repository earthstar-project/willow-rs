pub mod path;

/// Creates a statically-known [`Component`](prelude::Component).
///
/// Use this macro when you need to create a statically known path component. You can specify the component as ascii:
///
/// ```
/// # use willow25::prelude::*;
/// assert_eq!(component!("abc123-._~").as_ref(), b"abc123-._~");
/// ```
///
/// Bytes which are neither ascii alphanumerics nor the ascii encoding of one of `-._~` must be [percent-encoded](https://datatracker.ietf.org/doc/html/rfc3986#section-2.1): to encode the byte with hex representation `xy` (where `x` and `y` are ascii hex digits), write `%xy`. For example, to encode a forward slash (`/`), use `%2f`:
///
/// ```
/// # use willow25::prelude::*;
/// assert_eq!(component!("abc%2fdef").as_ref(), b"abc/def");
/// ```
///
/// The macro causes a compile-time error if the supplied component is invalid.
///
/// ```compile_fail
/// # use willow25::prelude::*;
/// // Component contains a character which must be percent-encoded.
/// let nope = component!(":");
/// ```
///
/// ```compile_fail
/// # use willow25::prelude::*;
/// // Component contains an invalid percent-encoding.
/// let nope = component!("%az");
/// ```
///
/// ```compile_fail
/// # use willow25::prelude::*;
/// // Component contains a character which must be percent-encoded.
/// let nope = component!(":");
/// ```
///
/// ```compile_fail
/// # use willow25::prelude::*;
/// // Component length must not exceed 4096 bytes.
/// let nope = component!("too_loooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong");
/// ```
#[macro_export]
macro_rules! component {
    ( $l:literal ) => {
        $crate::prelude::Component::new($crate::component_internal!($l)).unwrap()
    };
}

/// This is an implementation detail of a macro. Scoping things is a bit awkward in the world of macros sometimes. Please just pretend you never saw this, okay?
#[doc(hidden)]
pub use willow25_macros::component_internal;

/// A “prelude” for crates using the `willlow25` crate.
///
/// This prelude is similar to the standard library’s prelude in that you’ll almost always want to import its entire contents, but unlike the standard library’s prelude you’ll have to do so manually:
///
/// `use willow25::prelude::*;`
///
/// The prelude may grow over time.
pub mod prelude {
    // pub use super::entry::{Entry, EntryBuilder, Entrylike, EntrylikeExt, Keylike, Timestamp};
    // pub use super::ordering::{Maximum, Minimum, Successor};
    pub use super::path::*;
    pub use super::{MCC, MCL, MPL};
    pub use willow_data_model::prelude::{
        InvalidComponentError, Maximum, Minimum, PathError, PathFromComponentsError, Successor,
    };
}

/// The [**m**ax\_**c**omponent\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_component_length) of [Willow’25](https://macromania--macromania.deno.dev/specs/willow25/index.html#willow25_data_model): `4096`.
pub const MCL: usize = 4096;

/// The [**m**ax\_**c**omponent\_**c**count](https://willowprotocol.org/specs/data-model/index.html#max_component_count) of [Willow’25](https://macromania--macromania.deno.dev/specs/willow25/index.html#willow25_data_model): `4096`.
pub const MCC: usize = 4096;

/// The [**m**ax\_**p**ath\_**l**ength](https://willowprotocol.org/specs/data-model/index.html#max_path_length) of [Willow’25](https://macromania--macromania.deno.dev/specs/willow25/index.html#willow25_data_model): `4096`.
pub const MPL: usize = 4096;
