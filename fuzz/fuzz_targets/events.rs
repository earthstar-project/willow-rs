#![no_main]

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use either::Either::{Left, Right};
use libfuzzer_sys::fuzz_target;
use willow_fuzz::{
    placeholder_params::{
        FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
    },
    store::{check_store_equality, ControlStore, QueryIgnoreParams, StoreOp},
};
use willow_store_simple_sled::StoreSimpleSled;

// Generates a random area and ignore params, then creates a subscriber for those parameters on a fresh store. Runs a random sequence of operations on the store. The subscriber updates a collection of entries based on the events it receives. Once all ops have run, we query the store with the same area and ignore params which were used for the subscription. We assert that the query result is equal to the colleciton tha tthe subscriber reconstructed from the events it received.
fuzz_target!(|data: (
    FakeNamespaceId,
    Vec<(StoreOp, bool)>,
    Area,
    QueryIgnoreParams,
)| {
    pollster::block_on(async {
        let (namespace, ops, area, ignores) = data;

        let mut store = ControlStore::<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >::new(namespace.clone(), 1024);

        let mut sub = store.subscribe_area(area, ignores).await;

        let ((), collected) = futures::future::join(
            async {
                for (op, do_yield) in ops {
                    match op {
                        StoreOp::IngestEntry {
                            authorised_entry,
                            prevent_pruning,
                            origin,
                        } => {
                            if authorised_entry.entry().namespace_id() != namespace_id {
                                continue;
                            } else {
                                let _ = store
                                    .ingest_entry(
                                        authorised_entry.clone(),
                                        *prevent_pruning,
                                        *origin,
                                    )
                                    .await;
                            }
                        }
                        StoreOp::AppendPayload {
                            subspace,
                            path,
                            expected_digest,
                            data,
                        } => {
                            let mut payload_1 = FromSlice::new(data);

                            let _ = store
                                .append_payload(
                                    subspace,
                                    path,
                                    expected_digest.clone(),
                                    &mut payload_1,
                                )
                                .await;
                        }
                        StoreOp::ForgetEntry {
                            subspace_id,
                            path,
                            expected_digest,
                        } => {
                            let _ = store
                                .forget_entry(subspace_id, path, expected_digest.clone())
                                .await;
                        }
                        StoreOp::ForgetArea { area, protected } => {
                            let _ = store.forget_area(area, protected.as_ref()).await;
                        }
                        StoreOp::ForgetPayload {
                            subspace_id,
                            path,
                            expected_digest,
                        } => {
                            let _ = store
                                .forget_payload(subspace_id, path, expected_digest.clone())
                                .await;
                        }
                        StoreOp::ForgetAreaPayloads { area, protected } => {
                            let _ = store.forget_area_payloads(area, protected.as_ref()).await;
                        }
                        StoreOp::GetPayload { .. }
                        | StoreOp::GetEntry { .. }
                        | StoreOp::QueryArea { .. } => {
                            // no-op
                        }
                    }

                    if do_yield {
                        YieldOnce::new().await
                    }
                }
            },
            async {
                let mut collected = ControlStore::<
                    16,
                    16,
                    16,
                    FakeNamespaceId,
                    FakeSubspaceId,
                    FakePayloadDigest,
                    FakeAuthorisationToken,
                >::new(namespace.clone(), 1024);

                loop {
                    match sub.produce().await {
                        Err(_) => unreachable!(),
                        Ok(Right(())) => break,
                        Ok(Left(event)) => match event {
                            StoreEvent::Ingested { entry, origin } => {
                                let _ = collected
                                    .ingest_entry(entry.clone(), false, origin)
                                    .await
                                    .unwrap();
                            }
                            StoreEvent::Appended(lengthy_entry) => {
                                todo!()
                            }
                            StoreEvent::PruneAlert { cause } => {
                                todo!()
                            }
                            _ => todo!(),
                        },
                    }

                    todo!();
                }
            },
        )
        .await;
    });
});

struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(true)
    }
}

impl<'s> Future for MaybeYield<'s> {
    type Output = ();

    // The futures executor is implemented as a FIFO queue, so all this future
    // does is re-schedule the future back to the end of the queue, giving room
    // for other futures to progress.
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            self.0 = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
