//! Stable tiebreaker coverage.
//!
//! When two records have the same distance to the query, the result MUST
//! order them by their insertion order — the record inserted first wins
//! the tie. That keeps top-`k` deterministic across runs, across feature
//! flags, and across delete-then-reinsert sequences.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

#[test]
fn equal_distance_orders_by_insertion_order() {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
    idx.insert(VectorId::from(10u64), arc(&[1.0]), None)
        .unwrap();
    idx.insert(VectorId::from(20u64), arc(&[1.0]), None)
        .unwrap();
    idx.insert(VectorId::from(30u64), arc(&[1.0]), None)
        .unwrap();

    let hits = idx
        .search(&[0.0], &SearchParams::new(3, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, VectorId::U64(10));
    assert_eq!(hits[1].id, VectorId::U64(20));
    assert_eq!(hits[2].id, VectorId::U64(30));
    for hit in &hits {
        assert_eq!(hit.distance.to_bits(), 1.0_f32.to_bits());
    }
}

#[test]
fn delete_then_reinsert_moves_id_to_end_of_tiebreaker() {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
    idx.insert(VectorId::from(10u64), arc(&[1.0]), None)
        .unwrap();
    idx.insert(VectorId::from(20u64), arc(&[1.0]), None)
        .unwrap();
    idx.insert(VectorId::from(30u64), arc(&[1.0]), None)
        .unwrap();

    idx.delete(&VectorId::from(10u64)).unwrap();
    idx.insert(VectorId::from(10u64), arc(&[1.0]), None)
        .unwrap();

    let hits = idx
        .search(&[0.0], &SearchParams::new(3, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(
        hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
        vec![VectorId::U64(20), VectorId::U64(30), VectorId::U64(10),]
    );
}

#[test]
fn ties_are_consistent_across_runs() {
    let build = || {
        let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
        for id in [7u64, 3, 11, 1, 9] {
            idx.insert(VectorId::from(id), arc(&[2.0]), None).unwrap();
        }
        idx
    };
    let hits_a = build()
        .search(&[0.0], &SearchParams::new(3, DistanceMetric::Euclidean))
        .unwrap();
    let hits_b = build()
        .search(&[0.0], &SearchParams::new(3, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits_a, hits_b);
}
