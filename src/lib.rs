//! # iqdb-flat
//!
//! Brute-force **exact** nearest-neighbour search for the iQDB vector
//! database. [`FlatIndex`] scans every stored vector on every query — that
//! is the point. It is the ground-truth implementation that every
//! approximate index (HNSW, IVF) is measured against via recall@k, and it
//! is fast enough to be the right choice in its own right for small corpora
//! where graph or partition overhead is not justified.
//!
//! It is also the first and simplest consumer of the [`iqdb_index::Index`]
//! trait, so it doubles as that trait's design validation.
//!
//! ## Design
//!
//! - **Storage** is four parallel `Vec`s of length `len()` — the row
//!   payloads `Vec<Arc<[f32]>>`, the ids `Vec<VectorId>`, the optional
//!   per-row metadata `Vec<Option<Metadata>>`, and a monotonic
//!   insertion-sequence number `Vec<u64>` — plus a `HashMap<VectorId,
//!   usize>` mapping each live id to its current position. The map keeps
//!   the duplicate check on insert and the lookup on delete `O(1)`
//!   regardless of corpus size.
//! - **Zero-copy insert.** The row payload is an [`Arc<[f32]>`](std::sync::Arc)
//!   taken by value: [`FlatIndex`] stores the caller's `Arc` verbatim and
//!   never allocates a fresh `[f32]` buffer. A consumer that also keeps the
//!   vector in its own record store shares one underlying allocation with
//!   the index instead of paying for a copy.
//! - **One ordering invariant.** All distance math is delegated to
//!   [`iqdb_distance::compute_batch`] — flat never reimplements a metric.
//!   For [`DistanceMetric::DotProduct`](iqdb_types::DistanceMetric::DotProduct) the raw inner product (larger is
//!   more similar) is negated at the boundary so [`Hit::distance`] is always
//!   *smaller-is-nearer*, the same contract across all five metrics.
//! - **Bounded top-`k`.** Selection uses a max-heap of size `k` keyed by
//!   `(distance, seq)` via [`f32::total_cmp`], so it is `O(n log k)`,
//!   NaN-safe, and deterministic: ties break on insertion order (lower
//!   sequence number wins).
//! - **Filter-first.** A metadata filter is evaluated *before* distance
//!   computation, so a selective filter skips distance work in proportion
//!   to how much it rejects.
//!
//! ## Optional `parallel` feature
//!
//! The `parallel` feature adds a rayon-backed chunked distance scan for
//! large corpora. The sequential path is the correctness baseline; the
//! parallel path produces byte-identical results (enforced by
//! `tests/parallel_equivalence.rs`).
//!
//! ## Example
//!
//! ```
//! use std::sync::Arc;
//!
//! use iqdb_flat::{FlatConfig, FlatIndex};
//! use iqdb_index::{Index, IndexCore};
//! use iqdb_types::{DistanceMetric, SearchParams, VectorId};
//!
//! # fn main() -> iqdb_types::Result<()> {
//! let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig)?;
//! idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0][..]), None)?;
//! idx.insert(VectorId::from(2u64), Arc::<[f32]>::from(&[3.0, 4.0][..]), None)?;
//! idx.insert(VectorId::from(3u64), Arc::<[f32]>::from(&[1.0, 0.0][..]), None)?;
//!
//! let hits = idx.search(&[0.0, 0.0], &SearchParams::new(2, DistanceMetric::Euclidean))?;
//! assert_eq!(hits.len(), 2);
//! assert_eq!(hits[0].id, VectorId::U64(1));
//! assert_eq!(hits[1].id, VectorId::U64(3));
//! # Ok(())
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(warnings)]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unused_must_use)]
#![deny(unused_results)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::unreachable)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![forbid(unsafe_code)]

mod flat;
mod topk;

#[cfg(feature = "parallel")]
mod parallel;

pub use crate::flat::{FlatConfig, FlatIndex};

// Re-export the `Hit` type that searches return so callers can drive
// `FlatIndex` without a second `use` line for the result type.
pub use iqdb_types::Hit;

/// The version of this crate, taken from `Cargo.toml` at compile time.
///
/// # Examples
///
/// ```
/// let version = iqdb_flat::VERSION;
/// assert_eq!(version.split('.').count(), 3);
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
