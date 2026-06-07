//! The optional rayon-backed parallel scan (requires `--features parallel`).
//!
//! On a large corpus the distance scan is split into fixed-size chunks worked
//! in parallel. The result is **byte-identical** to the sequential baseline —
//! the same per-`(query, candidate)` kernel runs regardless of which thread
//! computes it — so turning the feature on changes throughput, not answers.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example parallel_scan --features parallel
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

const N: usize = 50_000;
const DIM: usize = 128;

fn main() -> Result<()> {
    let mut idx = FlatIndex::new(DIM, DistanceMetric::Euclidean, FlatConfig)?;
    let items: Vec<_> = (0..N as u64)
        .map(|i| {
            let v: Vec<f32> = (0..DIM).map(|j| ((i as usize + j) as f32).sin()).collect();
            (VectorId::from(i), arc(&v), None)
        })
        .collect();
    idx.insert_batch(items)?;

    let query: Vec<f32> = (0..DIM).map(|j| (j as f32).cos()).collect();
    let hits = idx.search(&query, &SearchParams::new(10, DistanceMetric::Euclidean))?;

    println!(
        "parallel scan over {N} × {DIM}-d vectors → top {} hits:",
        hits.len()
    );
    for (rank, hit) in hits.iter().enumerate() {
        println!("  #{rank}: id={} distance={:.5}", hit.id, hit.distance);
    }
    assert_eq!(hits.len(), 10);
    for w in hits.windows(2) {
        assert!(w[0].distance <= w[1].distance);
    }

    Ok(())
}
