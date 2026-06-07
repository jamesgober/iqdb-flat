//! The shortest end-to-end use of `iqdb-flat`: build, insert, search, inspect.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example quick_start
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn main() -> Result<()> {
    // A 2-D Euclidean index. `FlatConfig` is a unit struct — nothing to tune.
    let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig)?;

    idx.insert(VectorId::from(1u64), arc(&[0.0, 0.0]), None)?;
    idx.insert(VectorId::from(2u64), arc(&[3.0, 4.0]), None)?;
    idx.insert(VectorId::from(3u64), arc(&[1.0, 0.0]), None)?;

    let hits = idx.search(
        &[0.0, 0.0],
        &SearchParams::new(2, DistanceMetric::Euclidean),
    )?;

    println!("nearest 2 to (0, 0):");
    for (rank, hit) in hits.iter().enumerate() {
        println!("  #{rank}: id={} distance={:.3}", hit.id, hit.distance);
    }

    // Exact and ordered best-first.
    assert_eq!(hits[0].id, VectorId::U64(1)); // distance 0.0
    assert_eq!(hits[1].id, VectorId::U64(3)); // distance 1.0

    // A runtime snapshot.
    let stats = idx.stats();
    println!(
        "index_type={} n_vectors={} ~memory_bytes={}",
        stats.index_type, stats.n_vectors, stats.memory_bytes
    );
    assert_eq!(stats.index_type, "flat");
    assert_eq!(idx.len(), 3);

    Ok(())
}
