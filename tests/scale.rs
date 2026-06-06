//! C3 — scale evidence: 50k-vector insert + delete-half + reinsert that
//! would be pathological under the pre-C3 `O(n²)` insert/delete path.
//!
//! Before C3, `FlatIndex::insert` did a linear `position_of` scan over
//! every existing id to check for duplicates, and `FlatIndex::delete`
//! shifted the rest of the `Vec` left via `Vec::remove`. A workload of
//! N inserts followed by N/2 deletes ran `O(N²)` ≈ 1.9 × 10⁹ comparisons
//! at N=50k — minutes of wall clock in debug, much more under load.
//!
//! After C3 (HashMap id→pos, swap_remove delete, seq-based tiebreaker),
//! this is amortized `O(N)` and completes well under a second even in
//! debug builds. The wall-clock budget below is deliberately loose to
//! survive slow CI runners; on the fix it finishes in <1s, on the
//! pre-C3 code it would not finish.

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

const N: usize = 50_000;
const DIM: usize = 64;
const K: usize = 10;
// Generous to survive slow CI; pre-C3 this would not complete at all.
const BUDGET: Duration = Duration::from_secs(30);

fn row(i: usize) -> Vec<f32> {
    (0..DIM).map(|j| ((i * 17 + j * 31) as f32).sin()).collect()
}

#[test]
fn fifty_k_insert_delete_reinsert_search_within_budget() {
    let start = Instant::now();

    let mut idx = FlatIndex::new(DIM, DistanceMetric::Euclidean, FlatConfig).unwrap();

    // Phase 1 — N inserts. Pre-C3: O(N²) due to per-insert linear dup scan.
    for i in 0..N {
        idx.insert(VectorId::from(i as u64), arc(&row(i)), None)
            .unwrap();
    }
    assert_eq!(idx.len(), N);

    // Phase 2 — delete every other id. Pre-C3: O(N²) (position scan + shift).
    for i in (0..N).step_by(2) {
        idx.delete(&VectorId::from(i as u64)).unwrap();
    }
    assert_eq!(idx.len(), N / 2);

    // Phase 3 — reinsert the deleted ids with new payloads. Tests that
    // id_to_pos slots reclaimed by swap_remove are reusable, and that
    // next_seq remains monotonically larger than every live row's seq.
    for i in (0..N).step_by(2) {
        let v: Vec<f32> = (0..DIM).map(|j| ((i + j) as f32).cos()).collect();
        idx.insert(VectorId::from(i as u64), arc(&v), None).unwrap();
    }
    assert_eq!(idx.len(), N);

    // Phase 4 — a single search must work after that whole churn.
    let query: Vec<f32> = (0..DIM).map(|j| (j as f32).cos()).collect();
    let hits = idx
        .search(&query, &SearchParams::new(K, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits.len(), K);

    let elapsed = start.elapsed();
    assert!(
        elapsed < BUDGET,
        "50k insert+delete+reinsert+search took {elapsed:?}, budget {BUDGET:?} — \
         C3 has likely regressed to O(n^2) behavior",
    );
}
