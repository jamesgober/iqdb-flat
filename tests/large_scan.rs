//! Large-scan exact-correctness (v0.5.0).
//!
//! `FlatIndex` is the recall oracle for the whole iQDB index family — its
//! results are *defined* to be correct, and every approximate index is
//! measured against them. This test pins that guarantee at scale: at
//! N = 20_000 it asserts flat's top-`k` is **bit-for-bit identical** to an
//! independent naive full scan, including the stable tiebreaker.
//!
//! ## Why this can assert *exact* equality
//!
//! `correctness.rs` and `parallel_equivalence.rs` allow a few ULPs of slack
//! because their `f32` data makes the SIMD kernel and the scalar reference
//! accumulate in different orders. Here the vectors are small **integers**
//! (`0..=16`), so every Manhattan and DotProduct distance is an exact `f32`
//! (well under 2^24) regardless of accumulation order. There is no roundoff
//! to tolerate, so the hit lists must match exactly — ids, distances, and
//! order.
//!
//! Vectors are inserted in id order `0..N`, so each row's insertion sequence
//! equals its id. The naive oracle breaks distance ties by id; flat breaks
//! them by insertion sequence. The two tiebreakers therefore agree, which is
//! what lets the full ordered list — not just the id *set* — be compared.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Hit, SearchParams, VectorId};

const N: usize = 20_000;
const DIM: usize = 32;
const K: usize = 50;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

/// Deterministic small-integer row. The modulus keeps every component in
/// `0..=16`, so all Manhattan / DotProduct sums stay exactly representable
/// in `f32`.
fn row(i: usize) -> Vec<f32> {
    (0..DIM).map(|j| ((i * 31 + j * 17) % 17) as f32).collect()
}

fn ref_manhattan(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

fn ref_dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Naive full-scan oracle: score every row, sort by `(distance, id)`, take
/// the first `k`. `DotProduct` is negated to match flat's smaller-is-nearer
/// contract.
fn naive_topk(metric: DistanceMetric, query: &[f32], rows: &[Vec<f32>], k: usize) -> Vec<Hit> {
    let mut scored: Vec<(u64, f32)> = rows
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let d = match metric {
                DistanceMetric::Manhattan => ref_manhattan(query, v),
                DistanceMetric::DotProduct => -ref_dot(query, v),
                other => {
                    panic!("large_scan oracle only covers Manhattan/DotProduct, not {other:?}")
                }
            };
            (i as u64, d)
        })
        .collect();
    scored.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    scored.truncate(k);
    scored
        .into_iter()
        .map(|(id, distance)| Hit {
            id: VectorId::from(id),
            distance,
            metadata: None,
        })
        .collect()
}

fn assert_exact(metric: DistanceMetric) {
    let rows: Vec<Vec<f32>> = (0..N).map(row).collect();
    let mut idx = FlatIndex::new(DIM, metric, FlatConfig).unwrap();
    for (i, v) in rows.iter().enumerate() {
        idx.insert(VectorId::from(i as u64), arc(v), None).unwrap();
    }

    let query = row(N + 7);
    let actual = idx.search(&query, &SearchParams::new(K, metric)).unwrap();
    let expected = naive_topk(metric, &query, &rows, K);

    assert_eq!(actual.len(), expected.len(), "{metric:?}: hit count");
    for (rank, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(a.id, e.id, "{metric:?}: id mismatch at rank {rank}");
        assert_eq!(
            a.distance.to_bits(),
            e.distance.to_bits(),
            "{metric:?}: distance mismatch at rank {rank} (sut={} ref={})",
            a.distance,
            e.distance,
        );
    }
}

#[test]
fn manhattan_top_k_is_exact_at_scale() {
    assert_exact(DistanceMetric::Manhattan);
}

#[test]
fn dot_product_top_k_is_exact_at_scale() {
    assert_exact(DistanceMetric::DotProduct);
}

#[test]
fn full_scan_returns_every_id_exactly_once() {
    // k == N: the result must be a permutation of all ids, with no
    // duplicates and none missing — the storage churn invariants hold at
    // scale, not just in the small unit tests.
    let rows: Vec<Vec<f32>> = (0..N).map(row).collect();
    let mut idx = FlatIndex::new(DIM, DistanceMetric::Manhattan, FlatConfig).unwrap();
    for (i, v) in rows.iter().enumerate() {
        idx.insert(VectorId::from(i as u64), arc(v), None).unwrap();
    }

    let hits = idx
        .search(&row(0), &SearchParams::new(N, DistanceMetric::Manhattan))
        .unwrap();
    assert_eq!(hits.len(), N);

    let mut ids: Vec<u64> = hits
        .iter()
        .map(|h| match &h.id {
            VectorId::U64(v) => *v,
            VectorId::Bytes(_) => panic!("test inserts only U64 ids"),
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..N as u64).collect::<Vec<_>>());

    // Distances are non-decreasing (best-first).
    for w in hits.windows(2) {
        assert!(w[0].distance.total_cmp(&w[1].distance) != std::cmp::Ordering::Greater);
    }
}
