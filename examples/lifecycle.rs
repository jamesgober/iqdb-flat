//! Insert / delete / re-insert, and the stable-tiebreaker contract.
//!
//! Deletion is `O(1)` (`swap_remove` internally), but query results never
//! depend on physical storage order: each row carries a monotonic insertion
//! stamp, and equal-distance ties are broken by it. A re-inserted id gets a
//! *fresh* (higher) stamp, so it moves to the end of any tie — exactly as if
//! it had been appended.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example lifecycle
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, IqdbError, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn order(idx: &FlatIndex) -> Vec<u64> {
    idx.search(&[0.0], &SearchParams::new(10, DistanceMetric::Euclidean))
        .expect("search ok")
        .into_iter()
        .map(|h| match h.id {
            VectorId::U64(v) => v,
            VectorId::Bytes(_) => unreachable!("example inserts only U64 ids"),
        })
        .collect()
}

fn main() -> Result<()> {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig)?;

    // Three vectors, all at distance 1.0 from the query → a three-way tie,
    // resolved by insertion order.
    for id in [10u64, 20, 30] {
        idx.insert(VectorId::from(id), arc(&[1.0]), None)?;
    }
    println!("inserted: {:?}", order(&idx));
    assert_eq!(order(&idx), vec![10, 20, 30]);

    // Delete the middle id. The remaining two keep their order.
    idx.delete(&VectorId::from(20u64))?;
    println!("after delete(20): {:?}", order(&idx));
    assert_eq!(order(&idx), vec![10, 30]);
    assert_eq!(idx.len(), 2);

    // Deleting a missing id is a typed error, not a panic.
    assert_eq!(idx.delete(&VectorId::from(99u64)), Err(IqdbError::NotFound));

    // Re-insert 20 — it now sorts LAST in the tie (fresh, higher stamp).
    idx.insert(VectorId::from(20u64), arc(&[1.0]), None)?;
    println!("after re-insert(20): {:?}", order(&idx));
    assert_eq!(order(&idx), vec![10, 30, 20]);

    Ok(())
}
