//! Bulk loading with `insert_batch`, and reading the index's `stats`.
//!
//! `insert_batch` reserves capacity up front, so loading a corpus in one call
//! avoids the incremental reallocations a per-item loop would trigger. It is
//! fail-fast: the first bad item returns an error and the inserts before it
//! remain.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example batch_and_stats
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, IqdbError, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn main() -> Result<()> {
    let mut idx = FlatIndex::new(4, DistanceMetric::Cosine, FlatConfig)?;

    // Load 1,000 vectors in a single reserved-capacity call.
    let items: Vec<_> = (0..1_000u64)
        .map(|i| {
            let v: Vec<f32> = (0..4).map(|j| ((i + j) as f32).sin()).collect();
            (VectorId::from(i), arc(&v), None)
        })
        .collect();
    idx.insert_batch(items)?;
    println!("after batch load: len = {}", idx.len());
    assert_eq!(idx.len(), 1_000);

    let stats = idx.stats();
    println!(
        "stats: index_type={} n_vectors={} disk_bytes={:?} ~memory_bytes={}",
        stats.index_type, stats.n_vectors, stats.disk_bytes, stats.memory_bytes
    );
    assert_eq!(stats.index_type, "flat");
    assert_eq!(stats.disk_bytes, None); // purely in-memory

    // Fail-fast: a wrong-dimension item stops the batch; prior inserts stay.
    let before = idx.len();
    let bad = vec![
        (VectorId::from(10_000u64), arc(&[1.0, 1.0, 1.0, 1.0]), None),
        (VectorId::from(10_001u64), arc(&[1.0]), None), // wrong dim → error
        (VectorId::from(10_002u64), arc(&[2.0, 2.0, 2.0, 2.0]), None),
    ];
    let err = idx.insert_batch(bad).unwrap_err();
    println!("batch fail-fast error: {err:?}; len = {}", idx.len());
    assert!(matches!(err, IqdbError::DimensionMismatch { .. }));
    assert_eq!(idx.len(), before + 1); // only the first item landed

    // Sanity search still works after all that.
    let hits = idx.search(
        &[0.0, 0.0, 0.0, 1.0],
        &SearchParams::new(3, DistanceMetric::Cosine),
    )?;
    assert_eq!(hits.len(), 3);

    Ok(())
}
