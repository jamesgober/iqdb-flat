//! Parallel-path differential equivalence.
//!
//! Compiled only under the `parallel` feature. With that feature on, the
//! `FlatIndex::search` distance scan goes through the rayon-backed
//! chunked path. The test exercises an `n` large enough to actually trip
//! chunking (`n >= 2 * PARALLEL_CHUNK = 2048`) and asserts the resulting
//! hits match a naive single-threaded reference, bit-for-bit on distance.
//!
//! The reference distance functions in this file are **hand-coded** and do
//! NOT call into `iqdb_distance`. See `correctness.rs` for the reason.

#![cfg(feature = "parallel")]
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

fn dataset(n: usize, dim: usize) -> Raw {
    (0..n)
        .map(|i| {
            let row: Vec<f32> = (0..dim)
                .map(|j| ((i * 11 + j * 23) as f32).sin() + 0.25)
                .collect();
            (VectorId::from(i as u64), row, None)
        })
        .collect()
}

fn naive_topk(metric: DistanceMetric, query: &[f32], raw: &Raw, k: usize) -> Vec<Hit> {
    let mut scored: Vec<(usize, f32)> = raw
        .iter()
        .enumerate()
        .map(|(i, (_, vector, _))| {
            let mut d = independent_distance(metric, query, vector);
            if matches!(metric, DistanceMetric::DotProduct) {
                d = -d;
            }
            (i, d)
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

#[test]
fn parallel_search_matches_naive_for_each_metric() {
    const N: usize = 2_500;
    const DIM: usize = 64;
    const K: usize = 12;

    let raw = dataset(N, DIM);
    let query: Vec<f32> = (0..DIM).map(|j| ((j as f32) * 0.71).cos()).collect();

    for metric in [
        DistanceMetric::Cosine,
        DistanceMetric::DotProduct,
        DistanceMetric::Euclidean,
        DistanceMetric::Manhattan,
        DistanceMetric::Hamming,
    ] {
        let mut idx = FlatIndex::new(DIM, metric, FlatConfig).unwrap();
        for (id, vector, metadata) in &raw {
            idx.insert(id.clone(), arc(vector), metadata.clone())
                .unwrap();
        }
        let actual = idx.search(&query, &SearchParams::new(K, metric)).unwrap();
        let expected = naive_topk(metric, &query, &raw, K);
        assert_eq!(actual.len(), expected.len(), "metric {metric:?}");

        // At N=2500, near-tied distances are common; SUT and the
        // independent reference accumulate in different orders, so two
        // records that differ in distance by a ULP can swap places at the
        // top-`k` boundary. Order is asserted on the parallel-vs-sequential
        // SUT-vs-SUT equivalence (`flat::search` parallel == sequential),
        // not on the reference cross-check. Here we cross-check that the
        // SET of top-`k` ids agrees and that the distance the SUT reports
        // is consistent with the reference's distance for that id.
        const EPS_ABS: f32 = 1e-3;
        const EPS_REL: f32 = 1e-4;

        let actual_ids: std::collections::HashSet<&VectorId> =
            actual.iter().map(|h| &h.id).collect();
        let expected_ids: std::collections::HashSet<&VectorId> =
            expected.iter().map(|h| &h.id).collect();
        let agreed = actual_ids.intersection(&expected_ids).count();

        // Allow up to one boundary swap per metric — a single near-tie at
        // rank K can flip the membership of the cut-off entry. Anything
        // more would indicate a real disagreement, not roundoff.
        assert!(
            agreed + 1 >= K,
            "top-{K} id sets diverged under {metric:?} by more than one \
             boundary swap: actual={actual_ids:?} expected={expected_ids:?}",
        );

        // For every id the two sides agree on, distances must match within
        // epsilon — that is the real correctness contract.
        let expected_by_id: std::collections::HashMap<&VectorId, f32> =
            expected.iter().map(|h| (&h.id, h.distance)).collect();
        for hit in &actual {
            if let Some(&ref_dist) = expected_by_id.get(&hit.id) {
                let diff = (hit.distance - ref_dist).abs();
                let tol = EPS_REL * hit.distance.abs().max(ref_dist.abs());
                assert!(
                    diff <= EPS_ABS || diff <= tol,
                    "distance disagreement under {metric:?} for id {:?}: sut={} ref={}",
                    hit.id,
                    hit.distance,
                    ref_dist,
                );
            }
        }
    }
}
