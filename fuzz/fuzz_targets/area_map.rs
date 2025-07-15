#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use willow_data_model::grouping::{Area, AreaMap};
use willow_fuzz::placeholder_params::FakeSubspaceId;

fuzz_target!(|data: (
    Area<16, 16, 16, FakeSubspaceId>,
    HashMap<u64, Area<16, 16, 16, FakeSubspaceId>>
)| {
    let (query_area, insertions) = data;

    let mut area_map = AreaMap::<16, 16, 16, FakeSubspaceId, u64>::default();

    for (value, area) in insertions.clone() {
        area_map.insert(&area, value);
    }

    match area_map.intersecting_values(&query_area) {
        Some(values) => {
            for value in values {
                match insertions.get(value) {
                    Some(area) => {
                        if area.intersection(&query_area).is_none() {
                            panic!("Query area did not intersect with value area. Value area: {area:?}, Query area: {query_area:?}");
                        }
                    }
                    None => panic!("A value which shouldn't be here appeared"),
                }
            }
        }
        None => {
            for (value, area) in &insertions {
                if let Some(_intersection) = area.intersection(&query_area) {
                    panic!("Result was missing from AreaMap::intersecting_values: value {:?}, area: {:?}, query: {:?}", value, area, query_area)
                }
            }
        }
    }

    match area_map.intersecting_pairs(&query_area) {
        Some(values) => {
            for (area, value) in values {
                match area.intersection(&query_area) {
                    Some(_) => {}
                    None => {
                        panic!("Query area did not intersect with value area. Value area: {area:?}, Query area: {query_area:?}");
                    }
                }

                match insertions.get(value) {
                    Some(og_area) => {
                        assert_eq!(&area, og_area);
                    }
                    None => panic!("A value which shouldn't be here appeared"),
                }
            }
        }
        None => {
            for (value, area) in insertions {
                if let Some(_intersection) = area.intersection(&query_area) {
                    panic!("Result was missing from AreaMap::intersecting_values: value {:?}, area: {:?}, query: {:?}", value, area, query_area)
                }
            }
        }
    }
});
