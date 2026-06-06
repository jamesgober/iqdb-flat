//! `FlatIndex::search` filter-None path: allocation count is independent
//! of corpus size (audit finding M3).
//!
//! Pre-M3 the search hot path collected `(0..N)` into a `Vec<usize>`
//! even when no filter was supplied, plus a parallel `Vec<u64>` of
//! sequence numbers — two N-sized allocations per call that the
//! filter-None code never needs. Post-M3 the filter-None path iterates
//! `self.vectors` and `self.seqs` directly; only the unavoidable
//! `candidates: Vec<&[f32]>`, the `distances: Vec<f32>`, the top-`k`
//! chosen Vec, and the returned `Vec<Hit>` are allocated.
//!
//! What this test asserts: the *number of allocation calls* made by a
//! filter-None search is the same for N=128 and N=8192. That invariant
//! is N-independent both before and after the fix — pre-fix the count
//! is constant but high, post-fix the count is constant but lower. The
//! test does not RED-then-GREEN the M3 fix on its own (the search
//! benchmark proves the per-call byte reduction); what it locks in is
//! the contract that NO FUTURE CHANGE can introduce a per-row
//! allocation into the filter-None path. A `.iter().filter().collect()`
//! sneaking in there would scale the count with N and break this test.
//!
//! Gated on the sequential search path (`not(feature = "parallel")`):
//! rayon's chunk-and-fold allocates a small handful of bookkeeping
//! buffers that are work-stealing-shape-dependent, not row-count-
//! dependent — measuring it here would add noise unrelated to the
//! invariant under test. The default-features CI leg exercises this
//! file; the parallel leg is covered by `parallel_equivalence.rs`.

#![cfg(not(feature = "parallel"))]
#![allow(clippy::unwrap_used)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

/// Counter that ticks once per `alloc` call.
///
/// `Relaxed` is sufficient: the counter is observed before and after a
/// single-threaded search call; no cross-thread ordering invariant is
/// being established.
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Gate the counter so test setup (corpus build, warmup) does not
/// inflate the measurement.
static COUNTING: AtomicBool = AtomicBool::new(false);

struct CountingAlloc;

// SAFETY: This wrapper forwards every allocation request to the system
// allocator. The system allocator already upholds the `GlobalAlloc`
// contract; the wrapper only increments a counter on the way through
// (and only when the `COUNTING` gate is on), which is a side effect with
// no aliasing or alignment implications. `Relaxed` atomic ops are sound
// here.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            let _ = ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: forwarding the caller's layout unchanged to the
        // system allocator; the system allocator's `alloc` upholds the
        // `GlobalAlloc` safety contract for any valid `Layout`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` are forwarded unchanged from the
        // caller; the system allocator's `dealloc` upholds the
        // `GlobalAlloc` safety contract for any pointer/layout pair the
        // caller is permitted to free.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn build_index(n: usize) -> FlatIndex {
    let mut idx = FlatIndex::new(8, DistanceMetric::Euclidean, FlatConfig).unwrap();
    for i in 0..n {
        let v: Vec<f32> = (0..8).map(|j| (i as f32 + j as f32) * 0.01).collect();
        idx.insert(VectorId::from(i as u64), arc(&v), None).unwrap();
    }
    idx
}

fn count_search_allocs(idx: &FlatIndex, query: &[f32], params: &SearchParams) -> usize {
    // Note: the global allocator hook also catches any allocation the
    // test harness or stdlib performs during the `search` call (e.g.
    // panic-info buffers, assert formatting). Those allocations are
    // expected to be N-independent so they wash out of the
    // small-vs-large comparison; if cargo's test runner ever starts
    // doing per-iteration bookkeeping that scales with internal state,
    // this test could spuriously diverge. The warmup calls in the
    // caller cover one-shot lazy initialisation but not per-iteration
    // overhead inside the measured window.
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    let _hits = idx.search(query, params).unwrap();
    COUNTING.store(false, Ordering::Relaxed);
    ALLOC_COUNT.load(Ordering::Relaxed)
}

#[test]
fn filter_none_search_alloc_count_does_not_scale_with_n() {
    let small = build_index(128);
    let large = build_index(8_192);
    let query = vec![0.0_f32; 8];
    let params = SearchParams::new(10, DistanceMetric::Euclidean);

    // Warm: any first-call lazy initialisation inside the search path
    // (statics, OnceLock) must not be counted against the small index.
    let _ = small.search(&query, &params).unwrap();
    let _ = large.search(&query, &params).unwrap();

    let small_count = count_search_allocs(&small, &query, &params);
    let large_count = count_search_allocs(&large, &query, &params);

    assert_eq!(
        small_count, large_count,
        "filter-None search allocation count must be N-independent: \
         N=128 -> {small_count}, N=8192 -> {large_count}",
    );
}
