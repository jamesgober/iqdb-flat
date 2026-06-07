//! The same data under all five metrics — and the one ordering contract.
//!
//! `Hit.distance` is **smaller-is-nearer** for every metric. Cosine,
//! Euclidean, Manhattan, and Hamming satisfy that natively; `DotProduct` is a
//! *similarity* (larger is more similar), so flat negates it at the boundary so
//! the most-similar vector still sorts first. This example runs one query under
//! each metric and prints the nearest neighbour.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example metric_tour
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

const ROWS: &[(u64, [f32; 4])] = &[
    (1, [1.0, 0.0, 0.0, 0.0]),
    (2, [0.0, 1.0, 0.0, 0.0]),
    (3, [2.0, 0.0, 0.0, 0.0]),
    (4, [1.0, 1.0, 1.0, 1.0]),
];

fn main() -> Result<()> {
    let query = [1.0_f32, 0.0, 0.0, 0.0];

    for metric in [
        DistanceMetric::Cosine,
        DistanceMetric::DotProduct,
        DistanceMetric::Euclidean,
        DistanceMetric::Manhattan,
        DistanceMetric::Hamming,
    ] {
        let mut idx = FlatIndex::new(4, metric, FlatConfig)?;
        for (id, v) in ROWS {
            idx.insert(VectorId::from(*id), arc(v), None)?;
        }

        let hits = idx.search(&query, &SearchParams::new(4, metric))?;

        // Best-first holds for every metric: distances never decrease.
        for w in hits.windows(2) {
            assert!(w[0].distance <= w[1].distance);
        }

        println!(
            "{metric:?}: nearest id={} distance={:.4}",
            hits[0].id, hits[0].distance
        );
    }

    Ok(())
}
