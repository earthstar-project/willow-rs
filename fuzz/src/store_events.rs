use std::{
    cell::RefCell,
    collections::HashSet,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use crate::{
    placeholder_params::{
        FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
    },
    store::{ControlStore, StoreOp},
};
use either::Either::{Left, Right};
use ufotofu::{producer::FromSlice, Producer};
use willow_data_model::{
    grouping::Area, LengthyAuthorisedEntry, QueryIgnoreParams, Store, StoreEvent,
};

/// A function for testing whether a store correctly emits events. Panics if it doesn't.
pub fn check_store_events(
    namespace_id: FakeNamespaceId,
    ops: Vec<(
        StoreOp<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >,
        bool,
    )>,
    area: Area<16, 16, 16, FakeSubspaceId>,
    ignores: QueryIgnoreParams,
) {
    pollster::block_on(async {
        let store = ControlStore::<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >::new(namespace_id.clone(), 1024);

        let done = Rc::new(RefCell::new(false));

        let mut sub = store.subscribe_area(&area, ignores).await;

        let ((), collected) = futures::future::join(
            async {
                for (op, do_yield) in ops {
                    match op {
                        StoreOp::IngestEntry {
                            authorised_entry,
                            prevent_pruning,
                            origin,
                        } => {
                            if authorised_entry.entry().namespace_id() != &namespace_id {
                                continue;
                            } else {
                                let _ = store
                                    .ingest_entry(authorised_entry.clone(), prevent_pruning, origin)
                                    .await;
                            }
                        }
                        StoreOp::AppendPayload {
                            subspace,
                            path,
                            expected_digest,
                            data,
                        } => {
                            let mut payload_1 = FromSlice::new(&data);

                            let _ = store
                                .append_payload(
                                    &subspace,
                                    &path,
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
                                .forget_entry(&subspace_id, &path, expected_digest.clone())
                                .await;
                        }
                        StoreOp::ForgetArea { area, protected } => {
                            let _ = store.forget_area(&area, protected.as_ref()).await;
                        }
                        StoreOp::ForgetPayload {
                            subspace_id,
                            path,
                            expected_digest,
                        } => {
                            let _ = store
                                .forget_payload(&subspace_id, &path, expected_digest.clone())
                                .await;
                        }
                        StoreOp::ForgetAreaPayloads { area, protected } => {
                            let _ = store.forget_area_payloads(&area, protected.as_ref()).await;
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

                YieldOnce::new().await;
                store.flush().await.unwrap();
                YieldOnce::new().await;
                store.debug_cancel_subscribers();

                *done.borrow_mut() = true;
            },
            async {
                let collected = ControlStore::<
                    16,
                    16,
                    16,
                    FakeNamespaceId,
                    FakeSubspaceId,
                    FakePayloadDigest,
                    FakeAuthorisationToken,
                >::new(namespace_id.clone(), 1024);

                while !*done.borrow() {
                    match sub.produce().await {
                        Err(_) => unreachable!(),
                        Ok(Right(())) => break,
                        Ok(Left(event)) => {
                            // println!("Public subscriber got event: {:?}", event);

                            match event {
                                StoreEvent::Ingested { entry, origin } => {
                                    let _ = collected
                                        .ingest_entry(entry.clone(), false, origin)
                                        .await
                                        .unwrap();
                                }
                                StoreEvent::EntryForgotten { entry } => {
                                    let _ = collected
                                        .forget_entry(
                                            entry.entry().subspace_id(),
                                            entry.entry().path(),
                                            None,
                                        )
                                        .await;
                                }
                                StoreEvent::AreaForgotten { area, protected } => {
                                    let _ = collected.forget_area(&area, protected.as_ref()).await;
                                }
                                StoreEvent::PayloadForgotten(entry) => {
                                    let _ = collected
                                        .forget_payload(
                                            entry.entry().subspace_id(),
                                            entry.entry().path(),
                                            None,
                                        )
                                        .await;
                                }
                                StoreEvent::AreaPayloadsForgotten { area, protected } => {
                                    let _ = collected
                                        .forget_area_payloads(&area, protected.as_ref())
                                        .await;
                                }
                                StoreEvent::Appended {
                                    entry,
                                    previous_available,
                                    now_available,
                                } => {
                                    println!("StoreEvent::Appended received by control: {:?}, prev_available {:?}, now_available {:?}", entry, previous_available, now_available);
                                    match store
                                        .payload(
                                            entry.entry().subspace_id(),
                                            entry.entry().path(),
                                            previous_available,
                                            None,
                                        )
                                        .await
                                    {
                                        Ok(new_payload_bytes_producer) => {
                                            let mut limited = ufotofu::producer::Limit::new(new_payload_bytes_producer, (now_available - previous_available) as usize);

                                            let _ = collected
                                                .append_payload(
                                                    entry.entry().subspace_id(),
                                                    entry.entry().path(),
                                                    None,
                                                    &mut limited,
                                                )
                                                .await;
                                        }
                                        Err(_) => { /* no-op */ }
                                    }
                                }
                                StoreEvent::PruneAlert { cause } => {
                                    collected.prune(&cause).await;
                                }
                            }
                        }
                    }
                }

                // println!("collected: {:?}", collected);

                collected
            },
        )
        .await;

        // If events were implemented correctly, then querying the original store for the area and ignores of the subscription yields the same set of entries as querying the collected store for *all* its entries.

        let full_area = Area::new_full();
        let mut producer1 = store.query_area(&area, ignores).await.unwrap();
        let mut producer2 = collected
            .query_area(&full_area, QueryIgnoreParams::default())
            .await
            .unwrap();

        let mut set1 = HashSet::<
            LengthyAuthorisedEntry<
                16,
                16,
                16,
                FakeNamespaceId,
                FakeSubspaceId,
                FakePayloadDigest,
                FakeAuthorisationToken,
            >,
        >::new();
        let mut set2 = HashSet::<
            LengthyAuthorisedEntry<
                16,
                16,
                16,
                FakeNamespaceId,
                FakeSubspaceId,
                FakePayloadDigest,
                FakeAuthorisationToken,
            >,
        >::new();

        loop {
            match producer1.produce().await {
                Ok(Left(entry)) => {
                    set1.insert(entry);
                }
                Ok(Right(_)) => break,
                Err(_) => panic!("QueryArea: Store producer error"),
            }
        }

        loop {
            match producer2.produce().await {
                Ok(Left(entry)) => {
                    set2.insert(entry);
                }
                Ok(Right(_)) => break,
                Err(_) => panic!("QueryArea: Store producer error"),
            }
        }

        assert_eq!(set1, set2)
    });
}

struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(true)
    }
}

impl Future for YieldOnce {
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
