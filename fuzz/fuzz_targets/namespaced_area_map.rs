#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use willow_data_model::grouping::{Area, NamespacedAreaMap};
use willow_fuzz::placeholder_params::{FakeNamespaceId, FakeSubspaceId};

fuzz_target!(|data: (
    (FakeNamespaceId, Area<16, 16, 16, FakeSubspaceId>),
    HashMap<u64, (FakeNamespaceId, Area<16, 16, 16, FakeSubspaceId>)>
)| {
    let (namespaced_query_area, insertions) = data;

    let mut namespaced_area_map =
        NamespacedAreaMap::<16, 16, 16, FakeNamespaceId, FakeSubspaceId, u64>::default();

    for (value, (namespace, area)) in insertions.clone() {
        namespaced_area_map.insert(&namespace, &area, value);
    }

    let (query_namespace, query_area) = namespaced_query_area;

    match namespaced_area_map.intersecting_values(&query_namespace, &query_area) {
        Some(values) => {
            for value in values {
                match insertions.get(value) {
                    Some((namespace, area)) => {
                        if namespace != &query_namespace {
                            panic!("Query area did not belong to namespace")
                        }

                        if area.intersection(&query_area).is_none() {
                            panic!("Query area did not intersect with value area. Value area: {area:?}, Query area: {query_area:?}");
                        }
                    }
                    None => panic!("A value which shouldn't be here appeared"),
                }
            }
        }
        None => {
            for (value, (namespace, area)) in &insertions {
                if namespace == &query_namespace {
                    if let Some(_intersection) = area.intersection(&query_area) {
                        panic!("Result was missing from AreaMap::intersecting_values: value {:?}, area: {:?}, query: {:?}", value, area, query_area)
                    }
                }
            }
        }
    }

    match namespaced_area_map.intersecting_pairs(&query_namespace, &query_area) {
        Some(values) => {
            for (area, value) in values {
                if area.intersection(&query_area).is_none() {
                    panic!("Query area did not intersect with value area. Value area: {area:?}, Query area: {query_area:?}");
                }

                match insertions.get(value) {
                    Some((og_namespace, og_area)) => {
                        assert_eq!(&query_namespace, og_namespace);
                        assert_eq!(&area, og_area);
                    }
                    None => panic!("A value which shouldn't be here appeared"),
                }
            }
        }
        None => {
            for (value, (namespace, area)) in insertions {
                if namespace == query_namespace {
                    if let Some(_intersection) = area.intersection(&query_area) {
                        panic!("Result was missing from AreaMap::intersecting_values: value {:?}, area: {:?}, query: {:?}", value, area, query_area)
                    }
                }
            }
        }
    }
});
