#![no_main]

use wgps::messages::{ReconciliationAnnounceEntries, ReconciliationSendEntry};
use willow_data_model::{grouping::Range3d, AuthorisedEntry};
use willow_fuzz::placeholder_params::{
    FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
};

use ufotofu_codec::{fuzz_relative_known_size, Blame};

use libfuzzer_sys::fuzz_target;

use core::num::NonZeroUsize;

use ufotofu::consumer::TestConsumer;

use ufotofu_codec::proptest::{
    assert_relative_basic_invariants, assert_relative_canonic_invariants,
    assert_relative_known_size_invariants,
};

fuzz_target!(|data: (
    (
        (FakeNamespaceId, Range3d<16, 16, 16, FakeSubspaceId>),
        AuthorisedEntry<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >,
    ),
    ReconciliationSendEntry<
        16,
        16,
        16,
        FakeNamespaceId,
        FakeSubspaceId,
        FakePayloadDigest,
        FakeAuthorisationToken,
    >,
    ReconciliationSendEntry<
        16,
        16,
        16,
        FakeNamespaceId,
        FakeSubspaceId,
        FakePayloadDigest,
        FakeAuthorisationToken,
    >,
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

    let namespace_id = (r.0).0;
    let range_3d = (r.0).1;
    let authed_entry = r.1;

    let actual_r = ((&namespace_id, &range_3d), &authed_entry);

    if t1.entry.entry().entry().namespace_id() == &namespace_id
        && t2.entry.entry().entry().namespace_id() == &namespace_id
        && t1.entry.entry().entry().namespace_id() == authed_entry.entry().namespace_id()
        && t2.entry.entry().entry().namespace_id() == authed_entry.entry().namespace_id()
        && authed_entry.entry().namespace_id() == &namespace_id
        && range_3d.includes_entry(t1.entry.entry().entry())
        && range_3d.includes_entry(t2.entry.entry().entry())
    {
        pollster::block_on(async {
            assert_relative_basic_invariants::<
                ReconciliationSendEntry<
                    16,
                    16,
                    16,
                    FakeNamespaceId,
                    FakeSubspaceId,
                    FakePayloadDigest,
                    FakeAuthorisationToken,
                >,
                (
                    (&FakeNamespaceId, &Range3d<16, 16, 16, FakeSubspaceId>),
                    &AuthorisedEntry<
                        16,
                        16,
                        16,
                        FakeNamespaceId,
                        FakeSubspaceId,
                        FakePayloadDigest,
                        FakeAuthorisationToken,
                    >,
                ),
                Blame,
            >(
                &actual_r,
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

            assert_relative_known_size_invariants(&actual_r, &t1).await;
        });
    }
});
// };

// fuzz_relative_known_size!(
//     ReconciliationSendEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>;
//     (
//         (&FakeNamespaceId, &Range3d<16, 16, 16, FakeSubspaceId>),
//         &AuthorisedEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>,
//     );
//     Blame;
//     |msg: &ReconciliationSendEntry<16, 16, 16, FakeSubspaceId>, range:  &(
//         (&FakeNamespaceId, &Range3d<16, 16, 16, FakeSubspaceId>),
//         &AuthorisedEntry<16, 16, 16, FakeNamespaceId, FakeSubspaceId, FakePayloadDigest, FakeAuthorisationToken>,
//     )| {
//         true
//     }

// );
