//! Differential correctness tests.
//!
//! For every metric, build a small deterministic dataset, insert it into
//! `FlatIndex`, and assert the returned `Vec<Hit>` matches a naive
//! reference implementation exactly (same ids, same distances, same
//! order).
//!
//! The reference distance functions in this file are **hand-coded** and do
//! NOT call into `iqdb_distance`. If they did, a bug in `iqdb_distance`
//! would be invisible to flat's correctness test because the system under
//! test and the reference would compute the same wrong value. The whole
//! point of this test is to cross-check flat's top-`k` plus the DotProduct
//! flip against an independent implementation of the math.

#![allow(clippy::unwrap_used)]

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Hit, Metadata, SearchParams, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

type Raw = Vec<(VectorId, Vec<f32>, Option<Metadata>)>;

// --- Independent scalar references (do NOT use iqdb_distance) -----------

fn ref_euclidean(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = x - y;
        acc += d * d;
    }
    acc.sqrt()
}

fn ref_manhattan(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        acc += (x - y).abs();
    }
    acc
}

fn ref_dot_product(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        acc += x * y;
    }
    acc
}

fn ref_cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na * nb).sqrt();
    if denom == 0.0 {
        return 1.0;
    }
    1.0 - dot / denom
}

fn ref_hamming(a: &[f32], b: &[f32]) -> f32 {
    let mut count = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        if x.to_bits() != y.to_bits() {
            count += 1;
        }
    }
    count as f32
}

fn independent_distance(metric: DistanceMetric, a: &[f32], b: &[f32]) -> f32 {
    match metric {
        DistanceMetric::Cosine => ref_cosine(a, b),
        DistanceMetric::DotProduct => ref_dot_product(a, b),
        DistanceMetric::Euclidean => ref_euclidean(a, b),
        DistanceMetric::Manhattan => ref_manhattan(a, b),
        DistanceMetric::Hamming => ref_hamming(a, b),
        // `DistanceMetric` is `#[non_exhaustive]`. This file hand-codes a
        // reference for the five metrics iqdb-flat supports today; a new
        // metric must grow an arm here before its differential test passes.
        other => panic!("no hand-coded reference distance for {other:?}"),
    }
}

// ------------------------------------------------------------------------

fn deterministic_dataset(n: usize, dim: usize) -> Raw {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let row: Vec<f32> = (0..dim)
            .map(|j| ((i * 17 + j * 31) as f32).sin() + 0.5)
            .collect();
        out.push((VectorId::from(i as u64), row, None));
    }
    out
}

fn build_index(metric: DistanceMetric, dim: usize, raw: &Raw) -> FlatIndex {
    let mut idx = FlatIndex::new(dim, metric, FlatConfig).unwrap();
    for (id, vector, metadata) in raw {
        idx.insert(id.clone(), arc(vector), metadata.clone())
            .unwrap();
    }
    idx
}

fn naive_topk(metric: DistanceMetric, query: &[f32], raw: &Raw, k: usize) -> Vec<Hit> {
    if k == 0 {
        return Vec::new();
    }
    let mut scored: Vec<(usize, f32)> = raw
        .iter()
        .enumerate()
        .map(|(i, (_, vector, _))| {
            // DotProduct: smaller-is-nearer requires negation at the boundary,
            // matching the contract every concrete index enforces.
            let mut distance = independent_distance(metric, query, vector);
            if matches!(metric, DistanceMetric::DotProduct) {
                distance = -distance;
            }
            (i, distance)
        })
        .collect();
    scored.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    scored.truncate(k);
    scored
        .into_iter()
        .map(|(i, distance)| Hit {
            id: raw[i].0.clone(),
            distance,
            metadata: raw[i].2.clone(),
        })
        .collect()
}

// Independent and SUT computations accumulate f32 additions in different
// orders (SIMD chunks + horizontal sum vs. one straight scalar loop), so
// the final result can differ by a few ULPs. Distance equality is checked
// with the same epsilon shape the SIMD differential test uses. Ids and
// metadata are compared exactly.
const EPS_ABS: f32 = 1e-3;
const EPS_REL: f32 = 1e-4;

fn close_enough(x: f32, y: f32) -> bool {
    if !x.is_finite() || !y.is_finite() {
        return x.to_bits() == y.to_bits();
    }
    let diff = (x - y).abs();
    diff <= EPS_ABS || diff <= EPS_REL * x.abs().max(y.abs())
}

fn assert_hits_equal(left: &[Hit], right: &[Hit]) {
    assert_eq!(left.len(), right.len(), "different hit counts");
    for (a, b) in left.iter().zip(right.iter()) {
        assert_eq!(a.id, b.id, "id mismatch");
        assert!(
            close_enough(a.distance, b.distance),
            "distances disagree: sut={} ref={}",
            a.distance,
            b.distance,
        );
        assert_eq!(a.metadata, b.metadata, "metadata mismatch");
    }
}

fn check_metric(metric: DistanceMetric) {
    const N: usize = 50;
    const DIM: usize = 16;
    const K: usize = 7;

    let raw = deterministic_dataset(N, DIM);
    let idx = build_index(metric, DIM, &raw);
    let query: Vec<f32> = (0..DIM).map(|j| ((j as f32) * 0.37).cos()).collect();
    let params = SearchParams::new(K, metric);

    let actual = idx.search(&query, &params).unwrap();
    let expected = naive_topk(metric, &query, &raw, K);
    assert_hits_equal(&actual, &expected);
}

#[test]
fn matches_naive_for_cosine() {
    check_metric(DistanceMetric::Cosine);
}

#[test]
fn matches_naive_for_dot_product() {
    check_metric(DistanceMetric::DotProduct);
}

#[test]
fn matches_naive_for_euclidean() {
    check_metric(DistanceMetric::Euclidean);
}

#[test]
fn matches_naive_for_manhattan() {
    check_metric(DistanceMetric::Manhattan);
}

#[test]
fn matches_naive_for_hamming() {
    check_metric(DistanceMetric::Hamming);
}

#[test]
fn dot_product_distance_is_negated_inner_product() {
    let dim = 3;
    let raw: Raw = vec![
        (VectorId::from(1u64), vec![1.0, 0.0, 0.0], None),
        (VectorId::from(2u64), vec![0.0, 1.0, 0.0], None),
        (VectorId::from(3u64), vec![10.0, 0.0, 0.0], None),
    ];
    let idx = build_index(DistanceMetric::DotProduct, dim, &raw);
    let query = vec![1.0, 0.0, 0.0];
    let hits = idx
        .search(&query, &SearchParams::new(3, DistanceMetric::DotProduct))
        .unwrap();
    assert_eq!(hits[0].id, VectorId::U64(3));
    assert_eq!(hits[0].distance.to_bits(), (-10.0_f32).to_bits());
    assert_eq!(hits[1].id, VectorId::U64(1));
    assert_eq!(hits[1].distance.to_bits(), (-1.0_f32).to_bits());
    assert_eq!(hits[2].id, VectorId::U64(2));
    // Negating raw 0.0 yields -0.0; `total_cmp` orders -0.0 < +0.0, which is
    // why id 2 still sits last (its value, -0.0, is "less" than nothing
    // else here but still corresponds to the largest raw inner product
    // tied at 0).
    assert_eq!(hits[2].distance.to_bits(), (-0.0_f32).to_bits());
}
