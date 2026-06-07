//! Flat as the **recall oracle** — its reason for existing.
//!
//! Approximate indexes (HNSW, IVF) trade exactness for speed: they examine only
//! part of the corpus, so they can miss true neighbours. To know *how often*
//! they miss, you need a source of truth — the exact top-`k`. That is flat.
//!
//! This example builds a full flat index (the ground truth) and a second flat
//! index over only a sampled subset of the corpus (a stand-in for an
//! approximate index that pruned the rest). It then measures **recall@k**:
//! the fraction of the true top-`k` that the "approximate" result recovered.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example recall_oracle
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Result, SearchParams, VectorId};

const N: usize = 2_000;
const DIM: usize = 16;
const K: usize = 20;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn row(i: usize) -> Vec<f32> {
    (0..DIM)
        .map(|j| ((i * 13 + j * 7) as f32).sin() + 0.5)
        .collect()
}

fn ids_of(hits: &[iqdb_flat::Hit]) -> HashSet<VectorId> {
    hits.iter().map(|h| h.id.clone()).collect()
}

fn main() -> Result<()> {
    let metric = DistanceMetric::Cosine;
    let query = row(N + 1);

    // Ground truth: an exact flat index over the entire corpus.
    let mut truth = FlatIndex::new(DIM, metric, FlatConfig)?;
    for i in 0..N {
        truth.insert(VectorId::from(i as u64), arc(&row(i)), None)?;
    }
    let true_hits = truth.search(&query, &SearchParams::new(K, metric))?;
    let true_ids = ids_of(&true_hits);

    // A stand-in "approximate" index: it only ever indexed every other vector
    // (as if a partitioned index had pruned half the space for this query).
    let mut approx = FlatIndex::new(DIM, metric, FlatConfig)?;
    for i in (0..N).step_by(2) {
        approx.insert(VectorId::from(i as u64), arc(&row(i)), None)?;
    }
    let approx_hits = approx.search(&query, &SearchParams::new(K, metric))?;
    let approx_ids = ids_of(&approx_hits);

    let recovered = true_ids.intersection(&approx_ids).count();
    let recall = recovered as f64 / K as f64;

    println!("true top-{K} (exact, from flat over all {N} vectors):");
    for hit in true_hits.iter().take(5) {
        println!("  id={} distance={:.4}", hit.id, hit.distance);
    }
    println!("...");
    println!("approximate recovered {recovered}/{K} → recall@{K} = {recall:.2}");

    // The oracle is exact, so its own recall against itself is perfect.
    let self_recall = true_ids.intersection(&ids_of(&true_hits)).count();
    assert_eq!(
        self_recall, K,
        "flat is exact: recall against itself is 1.0"
    );

    // The pruned index can only do worse-or-equal — never better than truth.
    assert!(recovered <= K);

    Ok(())
}
