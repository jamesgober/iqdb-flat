//! Error hardening: hostile and degenerate inputs must surface a clean
//! `Result` — never a panic, an overflow, or a stack blow-up.
//!
//! `FlatIndex` validates untrusted input at the boundary and bounds every
//! allocation. These tests pin that contract against the inputs most likely
//! to break a naive implementation: non-finite floats, pathological filters,
//! and absurd `k`.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, IqdbError, SearchParams, Value, VectorId};
use proptest::prelude::*;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

// --- Non-finite inputs --------------------------------------------------

#[test]
fn nan_and_inf_in_stored_vectors_do_not_panic() {
    let mut idx = FlatIndex::new(3, DistanceMetric::Euclidean, FlatConfig).unwrap();
    idx.insert(VectorId::from(1u64), arc(&[f32::NAN, 0.0, 0.0]), None)
        .unwrap();
    idx.insert(VectorId::from(2u64), arc(&[f32::INFINITY, 1.0, 2.0]), None)
        .unwrap();
    idx.insert(VectorId::from(3u64), arc(&[0.0, 0.0, 0.0]), None)
        .unwrap();

    // No panic; NaN distances sort last via total_cmp, so the finite-best
    // (id 3, distance 0) comes first and every id is returned.
    let hits = idx
        .search(
            &[0.0, 0.0, 0.0],
            &SearchParams::new(3, DistanceMetric::Euclidean),
        )
        .unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, VectorId::U64(3));
}

#[test]
fn nan_query_does_not_panic_and_is_deterministic() {
    let mut idx = FlatIndex::new(2, DistanceMetric::Cosine, FlatConfig).unwrap();
    for id in 0..10u64 {
        idx.insert(VectorId::from(id), arc(&[id as f32, 1.0]), None)
            .unwrap();
    }
    let q = [f32::NAN, f32::NEG_INFINITY];
    let a = idx
        .search(&q, &SearchParams::new(5, DistanceMetric::Cosine))
        .unwrap();
    let b = idx
        .search(&q, &SearchParams::new(5, DistanceMetric::Cosine))
        .unwrap();
    // Compare by id and by the raw distance *bits* — `Hit`'s derived `==`
    // would report NaN-distanced hits as unequal even when they are the same
    // run, since `NaN != NaN`. The point of the test is that the *ordering*
    // and bit-patterns are reproducible.
    let key = |hits: &[iqdb_flat::Hit]| -> Vec<(VectorId, u32)> {
        hits.iter()
            .map(|h| (h.id.clone(), h.distance.to_bits()))
            .collect()
    };
    assert_eq!(
        key(&a),
        key(&b),
        "a NaN query must still produce a deterministic result",
    );
}

// --- Pathological filters ----------------------------------------------

#[test]
fn over_deep_filter_returns_invalid_filter_not_stack_overflow() {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
    idx.insert(VectorId::from(1u64), arc(&[0.0]), None).unwrap();

    // Nest well past iqdb_filter::MAX_FILTER_DEPTH (64). A naive recursive
    // evaluator would blow the stack; the validated evaluator must reject it.
    let mut filter = Filter::eq("k", Value::Int(1));
    for _ in 0..(iqdb_filter::MAX_FILTER_DEPTH + 16) {
        filter = Filter::not(filter);
    }
    let params = SearchParams {
        filter: Some(filter),
        ..SearchParams::new(1, DistanceMetric::Euclidean)
    };
    let err = idx.search(&[0.0], &params).unwrap_err();
    assert_eq!(err, IqdbError::InvalidFilter);
}

// --- Absurd k -----------------------------------------------------------

#[test]
fn k_at_usize_max_returns_all_without_overflow() {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
    for id in 0..5u64 {
        idx.insert(VectorId::from(id), arc(&[id as f32]), None)
            .unwrap();
    }
    let hits = idx
        .search(
            &[0.0],
            &SearchParams::new(usize::MAX, DistanceMetric::Euclidean),
        )
        .unwrap();
    assert_eq!(hits.len(), 5, "k > n must clamp to n, not over-allocate");
}

// --- No-panic property --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// For arbitrary dimensions, arbitrary (possibly non-finite) vectors,
    /// arbitrary `k`, and an optional filter, `insert`/`search` either
    /// succeed or return a typed error — they never panic.
    #[test]
    fn search_never_panics_on_arbitrary_input(
        dim in 1usize..12,
        rows in proptest::collection::vec(
            proptest::collection::vec(
                prop_oneof![
                    any::<f32>(),
                    Just(f32::NAN),
                    Just(f32::INFINITY),
                    Just(f32::NEG_INFINITY),
                ],
                1..12,
            ),
            0..40,
        ),
        k in 0usize..64,
        with_filter in any::<bool>(),
    ) {
        let metric = DistanceMetric::Euclidean;
        let mut idx = FlatIndex::new(dim, metric, FlatConfig).unwrap();
        for (i, row) in rows.iter().enumerate() {
            if row.len() == dim {
                // Mismatched-dim rows are rejected by design; only feed
                // correctly-shaped ones so we exercise the search path.
                let _ = idx.insert(VectorId::from(i as u64), arc(row), None);
            }
        }
        let query: Vec<f32> = (0..dim).map(|j| j as f32 - 3.0).collect();
        let params = if with_filter {
            SearchParams {
                filter: Some(Filter::eq("missing", Value::Bool(true))),
                ..SearchParams::new(k, metric)
            }
        } else {
            SearchParams::new(k, metric)
        };
        // The contract under test: this returns, it does not panic.
        let result = idx.search(&query, &params);
        prop_assert!(result.is_ok());
        if let Ok(hits) = result {
            prop_assert!(hits.len() <= k);
        }
    }
}
