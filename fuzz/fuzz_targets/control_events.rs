#![no_main]

use libfuzzer_sys::fuzz_target;
use willow_data_model::{grouping::Area, QueryIgnoreParams};
use willow_fuzz::{
    placeholder_params::{
        FakeAuthorisationToken, FakeNamespaceId, FakePayloadDigest, FakeSubspaceId,
    },
    store::StoreOp,
    store_events::check_store_events,
};

// Generates a random area and ignore params, then creates a subscriber for those parameters on a fresh store. Runs a random sequence of operations on the store. The subscriber updates a collection of entries based on the events it receives. Once all ops have run, we query the store with the same area and ignore params which were used for the subscription. We assert that the query result is equal to the colleciton tha tthe subscriber reconstructed from the events it received.
fuzz_target!(|data: (
    FakeNamespaceId,
    Vec<(
        StoreOp<
            16,
            16,
            16,
            FakeNamespaceId,
            FakeSubspaceId,
            FakePayloadDigest,
            FakeAuthorisationToken,
        >,
        bool
    )>,
    Area<16, 16, 16, FakeSubspaceId>,
    QueryIgnoreParams,
)| {
    let (namespace_id, ops, area, ignores) = data;

    check_store_events(namespace_id, ops, area, ignores);
});
