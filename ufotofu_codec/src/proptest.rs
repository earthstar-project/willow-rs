//! Assertion functions that panic when their arguments certify a violation of the contract of a codec trait. Used by the fuzz macros, but also exposed here so that they can be used in non-fuzz-based property testing settings.

mod absolute;
pub use absolute::*;

mod relative;
pub use relative::*;

//----------//
// Absolute //
//----------//

/// A macro for running fuzz tests that check the invariants of all absolute codec traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_all;
///
/// fuzz_absolute_all!(path::to_some::TypeToTest);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_absolute_all {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_size_invariants,
        };

        fuzz_target!(|data: (
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                potential_encoding2,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_basic_invariants(
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_known_size_invariants(&t1).await;

                assert_canonic_invariants::<$t>(&potential_encoding1, &potential_encoding2).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`Encodable`](crate::Encodable), [`Decodable`](crate::Decodable), and [`DecodableCanonic`](crate::DecodableCanonic) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_canonic;
///
/// fuzz_absolute_canonic!(path::to_some::TypeToTest);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_absolute_canonic {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_size_invariants,
        };

        fuzz_target!(|data: (
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                potential_encoding2,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_basic_invariants(
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_canonic_invariants::<$t>(&potential_encoding1, &potential_encoding2).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`Encodable`](crate::Encodable), [`EncodableKnownSize`](crate::EncodableKnownSize), and [`Decodable`](crate::Decodable) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_known_size;
///
/// fuzz_absolute_known_size!(path::to_some::TypeToTest);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_absolute_known_size {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_size_invariants,
        };

        fuzz_target!(|data: (
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_basic_invariants(
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_known_size_invariants(&t1).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`Encodable`](crate::Encodable) and [`Decodable`](crate::Decodable) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_basic;
///
/// fuzz_absolute_basic!(path::to_some::TypeToTest);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_absolute_basic {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_size_invariants,
        };

        fuzz_target!(|data: (
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_basic_invariants(
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;
            });
        });
    };
}

//----------//
// Relative //
//----------//

/// A macro for running fuzz tests that check the invariants of all relative codec traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_relative_all;
///
/// fuzz_relative_all!(path::to_some::TypeToTest; path::to::RelativeToType; path::to::ErrorReason; path::to::ErrorCanonic);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_relative_all {
    ($t:ty; $r:ty; $errR:ty; $errC:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_relative_basic_invariants, assert_relative_canonic_invariants,
            assert_relative_known_size_invariants,
        };

        fuzz_target!(|data: (
            $r,
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                r,
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                potential_encoding2,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_relative_basic_invariants::<$t, $r, $errR>(
                    &r,
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_relative_known_size_invariants(&r, &t1).await;

                assert_relative_canonic_invariants::<$t, $r, $errR, $errC>(
                    &r,
                    &potential_encoding1,
                    &potential_encoding2,
                )
                .await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`RelativeEncodable`](crate::RelativeEncodable), [`RelativeDecodable`](crate::RelativeDecodable), and [`RelativeDecodableCanonic`](crate::RelativeDecodableCanonic) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_relative_canonic;
///
/// fuzz_relative_canonic!(path::to_some::TypeToTest; path::to::RelativeToType; path::to::ErrorReason; path::to::ErrorCanonic);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_relative_canonic {
    ($t:ty; $r:ty; $errR:ty; $errC:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_relative_basic_invariants, assert_relative_canonic_invariants,
            assert_relative_known_size_invariants,
        };

        fuzz_target!(|data: (
            $r,
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                r,
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                potential_encoding2,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_relative_basic_invariants::<$t, $r, $errR>(
                    &r,
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_relative_canonic_invariants::<$t, $r, $errR, $errC>(
                    &r,
                    &potential_encoding1,
                    &potential_encoding2,
                )
                .await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`RelativeEncodable`](crate::RelativeEncodable), [`RelativeEncodableKnownSize`](crate::RelativeEncodableKnownSize), and [`RelativeDecodable`](crate::RelativeDecodable) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_relative_known_size;
///
/// fuzz_relative_known_size!(path::to_some::TypeToTest; path::to::RelativeToType; path::to::ErrorReason);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_relative_known_size {
    ($t:ty; $r:ty; $errR:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_relative_basic_invariants, assert_relative_canonic_invariants,
            assert_relative_known_size_invariants,
        };

        fuzz_target!(|data: (
            $r,
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                r,
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_relative_basic_invariants::<$t, $r, $errR>(
                    &r,
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;

                assert_relative_known_size_invariants(&r, &t1).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`RelativeEncodable`](crate::RelativeEncodable) and [`RelativeDecodable`](crate::RelativeDecodable) traits. Usage:
///
/// ```ignore
/// #![no_main]
///
/// use ufotofu_codec::fuzz_relative_basic;
///
/// fuzz_relative_basic!(path::to_some::TypeToTest; path::to::RelativeToType; path::to::ErrorReason);
/// ```
///
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export]
macro_rules! fuzz_relative_basic {
    ($t:ty; $r:ty; $errR:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_relative_basic_invariants, assert_relative_canonic_invariants,
            assert_relative_known_size_invariants,
        };

        fuzz_target!(|data: (
            $r,
            $t,
            $t,
            TestConsumer<u8, (), ()>,
            TestConsumer<u8, (), ()>,
            Box<[u8]>,
            Box<[NonZeroUsize]>,
            Box<[NonZeroUsize]>,
            Box<[bool]>,
            Box<[bool]>,
        )| {
            let (
                r,
                t1,
                t2,
                c1,
                c2,
                potential_encoding1,
                exposed_items_sizes1,
                exposed_items_sizes2,
                yield_pattern1,
                yield_pattern2,
            ) = data;

            pollster::block_on(async {
                assert_relative_basic_invariants::<$t, $r, $errR>(
                    &r,
                    &t1,
                    &t2,
                    c1,
                    c2,
                    &potential_encoding1,
                    exposed_items_sizes1,
                    exposed_items_sizes2,
                    yield_pattern1,
                    yield_pattern2,
                )
                .await;
            });
        });
    };
}
