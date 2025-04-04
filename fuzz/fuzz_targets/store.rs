#![no_main]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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
    [Vec<
        StoreOp<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >,
    >; 1]
)| {
    // println!("🏁 ");

    let things_are_fine = Arc::new(AtomicBool::new(false));

    let thread_thing = things_are_fine.clone();

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1_000));
        if !thread_thing.load(Ordering::Relaxed) {
            std::process::exit(-1);
        }
    });

    let (namespace, op_sequences) = data;

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

    for op_sequence in op_sequences {
        let mut control_store = ControlStore::<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >::new(namespace.clone(), 1024);

        //  println!("y...");
        pollster::block_on(async {
            check_store_equality(&mut control_store, &mut sled_store, &op_sequence).await
        });

        sled_store.clear().unwrap();
        //  println!("...es.");
    }

    // println!("✅");

    /*
    for op_sequence in op_sequences {
        let mut control_store = ControlStore::<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >::new(namespace.clone(), 1024);
        let mut control_store2 = ControlStore::<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >::new(namespace.clone(), 1024);

        println!("y...");
        pollster::block_on(async {
            check_store_equality(&mut control_store, &mut control_store2, &op_sequence).await
        });
        println!("...es.");
    }
    */

    things_are_fine.store(true, Ordering::Relaxed);
});
