#![no_main]

use libfuzzer_sys::fuzz_target;
use tempdir::TempDir;
use willow_fuzz::{
    placeholder_params::{
        FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
    },
    store::{check_store_equality, ControlStore, StoreOp},
};
use willow_store_simple_sled::StoreSimpleSled;

fuzz_target!(|data: (
    FakeNamespaceId,
    Vec<
        StoreOp<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >,
    >
)| {
    let (namespace, ops) = data;

    let mut control_store = ControlStore::<
        16,
        16,
        16,
        FakeNamespaceId,
        FakeSubspaceId,
        FakePayloadDigest,
        FakeAuthorisationToken,
    >::new(namespace.clone(), 1024);

    let tmp_dir = TempDir::new("fuzz").unwrap();
    let file_path = tmp_dir.path().join("db");
    let sled_db = sled::open(file_path).unwrap();

    let mut sled_store = StoreSimpleSled::<
        16,
        16,
        16,
        FakeNamespaceId,
        FakeSubspaceId,
        FakePayloadDigest,
        FakeAuthorisationToken,
    >::new(&namespace, sled_db)
    .unwrap();

    pollster::block_on(async {
        check_store_equality(&mut control_store, &mut sled_store, &ops).await
    })
});
