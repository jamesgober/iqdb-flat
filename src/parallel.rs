//! Rayon-backed parallel distance scan.
//!
//! The function is feature-gated to `parallel`; the sequential path in
//! [`crate::flat`] is the correctness baseline.
//!
//! ## Determinism
//!
//! The chunked computation is byte-identical to a single
//! [`iqdb_distance::compute_batch`] call over the same candidates: each
//! `(query, candidate)` pair is computed by the same per-metric kernel
//! regardless of which thread runs it, so the resulting `f32` bytes match
//! the sequential path exactly. The differential test
//! `tests/parallel_equivalence.rs` enforces this.

use iqdb_types::{DistanceMetric, Result};
use rayon::prelude::*;

/// Chunk size used by the parallel scan. Small inputs short-circuit to the
/// sequential path; large inputs split into fixed-size chunks for
/// even-ish load distribution without unbounded scheduler overhead.
const PARALLEL_CHUNK: usize = 1024;

/// Compute `metric(query, candidates[i])` into `out[i]` in parallel.
///
/// `out.len()` MUST equal `candidates.len()`; the underlying
/// [`iqdb_distance::compute_batch`] surface enforces that and returns
/// [`iqdb_types::IqdbError::InvalidConfig`] otherwise.
pub(crate) fn compute_distances(
    metric: DistanceMetric,
    query: &[f32],
    candidates: &[&[f32]],
    out: &mut [f32],
) -> Result<()> {
    if candidates.len() < PARALLEL_CHUNK * 2 {
        return iqdb_distance::compute_batch(metric, query, candidates, out);
    }
    out.par_chunks_mut(PARALLEL_CHUNK)
        .zip(candidates.par_chunks(PARALLEL_CHUNK))
        .try_for_each(|(out_chunk, cand_chunk)| {
            iqdb_distance::compute_batch(metric, query, cand_chunk, out_chunk)
        })
}
