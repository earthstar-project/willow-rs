mod absolute;
pub use absolute::*;

/// A macro for running fuzz tests that check the invariants of all absolute codec traits. Usage:
/// 
/// ```rust
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_all;
///
/// fuzz_absolute_all!(path::to_some::TypeToTest);
/// ```
/// 
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export] macro_rules! fuzz_absolute_all {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_length_invariants,
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
        
                assert_known_length_invariants(&t1).await;
        
                assert_canonic_invariants::<$t>(&potential_encoding1, &potential_encoding2).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`Encodable`], [`Decodable`], and [`DecodableCanonic`] traits. Usage:
/// 
/// ```rust
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_all;
///
/// fuzz_absolute_all!(path::to_some::TypeToTest);
/// ```
/// 
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export] macro_rules! fuzz_absolute_canonic {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_length_invariants,
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

/// A macro for running fuzz tests that check the invariants of the [`Encodable`], [`EncodableKnownSize`], and [`Decodable`] traits. Usage:
/// 
/// ```rust
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_all;
///
/// fuzz_absolute_all!(path::to_some::TypeToTest);
/// ```
/// 
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export] macro_rules! fuzz_absolute_known_length {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_length_invariants,
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
        
                assert_known_length_invariants(&t1).await;
            });
        });
    };
}

/// A macro for running fuzz tests that check the invariants of the [`Encodable`] and [`Decodable`] traits. Usage:
/// 
/// ```rust
/// #![no_main]
///
/// use ufotofu_codec::fuzz_absolute_all;
///
/// fuzz_absolute_all!(path::to_some::TypeToTest);
/// ```
/// 
/// Assumes that `libfuzzer_sys`, `ufotofu`, and `ufotofu_codec` modules are available.
#[macro_export] macro_rules! fuzz_absolute_basic {
    ($t:ty) => {
        use libfuzzer_sys::fuzz_target;

        use core::num::NonZeroUsize;

        use ufotofu::consumer::TestConsumer;

        use ufotofu_codec::proptest::{
            assert_basic_invariants, assert_canonic_invariants, assert_known_length_invariants,
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
            });
        });
    };
}