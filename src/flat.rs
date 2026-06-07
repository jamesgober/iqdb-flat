//! [`FlatIndex`] — brute-force exact nearest-neighbour search.
//!
//! Storage is four parallel `Vec`s of length `len()` (the row payload
//! `Vec<Arc<[f32]>>`, ids `Vec<VectorId>`, optional metadata
//! `Vec<Option<Metadata>>`, and insertion-sequence numbers `Vec<u64>`),
//! plus an `id_to_pos: HashMap<VectorId, usize>` that points at the
//! current Vec position of each live id.
//!
//! The row payload is wrapped in [`std::sync::Arc`] so a consumer that
//! also keeps the vector in its own record store can share a single
//! payload allocation with the index. `FlatIndex` pushes the `Arc` it
//! receives via [`IndexCore::insert`] verbatim; it never allocates a fresh
//! `[f32]` buffer of its own. Search paths reborrow each `Arc` as `&[f32]`
//! for the distance kernels, so the per-row distance loop is identical to a
//! plain `Vec<Vec<f32>>` layout while keeping the zero-copy insert.
//!
//! Insert and delete are both amortized `O(1)` against the corpus size:
//!
//! - **Insert.** Reject if `id_to_pos` already contains `id` (`Duplicate`);
//!   else push the row, id, metadata, and a fresh monotonic
//!   `seq = next_seq++` to the four parallel `Vec`s and record
//!   `id_to_pos[id] = len - 1`.
//! - **Delete.** Look up `pos = id_to_pos.remove(id)?`. `swap_remove` each
//!   of the four parallel `Vec`s at `pos`. If `pos < new_len`, the entry
//!   formerly at the back is now at `pos`; update its `id_to_pos` slot.
//!
//! The stable tiebreaker on top-`k` keys off the stored `seq`, not the
//! row's position, so `swap_remove`'s reordering does not change query
//! results. The "earlier-inserted-wins" semantic is preserved exactly: a
//! re-inserted id gets a fresh `next_seq`, which is larger than every
//! existing `seq`, and therefore sorts last in ties.
//!
//! A search scans every entry (subject to an optional metadata
//! pre-filter), computes the distance for each through
//! [`iqdb_distance::compute_batch`], normalises `DotProduct` to honour
//! `Hit.distance`'s "smaller is nearer" contract, selects the top-`k`
//! via [`crate::topk::select_topk_indices`] keyed by `(distance, seq)`,
//! and returns the chosen [`Hit`]s in best-first order.

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use iqdb_filter::FilterEvaluator;
use iqdb_index::{Index, IndexCore, IndexStats};
use iqdb_types::{
    DistanceMetric, Filter, Hit, IqdbError, Metadata, Result, SearchParams, VectorId,
};

use crate::topk;

/// Configuration for [`FlatIndex::new`].
///
/// Unit struct: the flat index has no tunable knobs. It exists so
/// [`FlatIndex`] satisfies [`iqdb_index::Index`]'s associated
/// [`Config`](iqdb_index::Index::Config) bound (`Default + Clone`), and so
/// future knobs (initial capacity, parallel chunk size) can land here
/// without changing the trait surface.
///
/// # Examples
///
/// ```
/// use iqdb_flat::FlatConfig;
///
/// let config = FlatConfig;
/// let _cloned = config.clone();
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlatConfig;

/// Brute-force exact nearest-neighbour index.
///
/// See the crate-level docs for the design notes and the
/// [`iqdb_index::IndexCore`] / [`iqdb_index::Index`] contracts this type
/// satisfies.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use iqdb_flat::{FlatConfig, FlatIndex};
/// use iqdb_index::{Index, IndexCore};
/// use iqdb_types::{DistanceMetric, SearchParams, VectorId};
///
/// # fn main() -> iqdb_types::Result<()> {
/// let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig)?;
/// idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0][..]), None)?;
/// idx.insert(VectorId::from(2u64), Arc::<[f32]>::from(&[3.0, 4.0][..]), None)?;
///
/// let hits = idx.search(&[0.0, 0.0], &SearchParams::new(1, DistanceMetric::Euclidean))?;
/// assert_eq!(hits.len(), 1);
/// assert_eq!(hits[0].id, VectorId::U64(1));
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct FlatIndex {
    dim: usize,
    metric: DistanceMetric,
    vectors: Vec<Arc<[f32]>>,
    ids: Vec<VectorId>,
    metadata: Vec<Option<Metadata>>,
    /// Monotonic insertion-sequence number per row, parallel to `vectors`.
    /// Top-`k` selection tie-breaks on this — *not* on the row's position —
    /// so `swap_remove` does not change query results.
    seqs: Vec<u64>,
    /// Next sequence number to assign on insert. Monotonically increasing;
    /// never recycled. A re-inserted id therefore gets a fresh `seq` larger
    /// than every existing `seq` and sorts last in ties.
    next_seq: u64,
    /// Live id → current Vec position. Maintained on insert and on the
    /// `swap_remove` step of delete. Keeps duplicate checks and lookups
    /// `O(1)` regardless of corpus size.
    id_to_pos: HashMap<VectorId, usize>,
}

impl FlatIndex {
    /// Builds an empty index for `dim`-component vectors compared under
    /// `metric`.
    ///
    /// Returns [`IqdbError::InvalidConfig`] when `dim == 0`. This is the
    /// same construction surface as [`Index::new`]; calling it directly
    /// is the convenient path when the concrete type is known and there is
    /// nothing to configure.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_flat::FlatIndex;
    /// use iqdb_types::DistanceMetric;
    ///
    /// let idx = FlatIndex::new_unconfigured(3, DistanceMetric::Cosine)?;
    /// assert_eq!(idx.dim(), 3);
    /// assert!(idx.is_empty());
    /// # Ok::<(), iqdb_types::IqdbError>(())
    /// ```
    pub fn new_unconfigured(dim: usize, metric: DistanceMetric) -> Result<Self> {
        if dim == 0 {
            return Err(IqdbError::InvalidConfig {
                reason: "FlatIndex dim must be greater than zero",
            });
        }
        Ok(Self {
            dim,
            metric,
            vectors: Vec::new(),
            ids: Vec::new(),
            metadata: Vec::new(),
            seqs: Vec::new(),
            next_seq: 0,
            id_to_pos: HashMap::new(),
        })
    }

    /// The dimensionality the index was built for.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_flat::{FlatConfig, FlatIndex};
    /// use iqdb_index::Index;
    /// use iqdb_types::DistanceMetric;
    ///
    /// let idx = FlatIndex::new(8, DistanceMetric::Euclidean, FlatConfig)?;
    /// assert_eq!(idx.dim(), 8);
    /// # Ok::<(), iqdb_types::IqdbError>(())
    /// ```
    #[must_use]
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// The distance metric the index was built for.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_flat::{FlatConfig, FlatIndex};
    /// use iqdb_index::Index;
    /// use iqdb_types::DistanceMetric;
    ///
    /// let idx = FlatIndex::new(8, DistanceMetric::Cosine, FlatConfig)?;
    /// assert_eq!(idx.metric(), DistanceMetric::Cosine);
    /// # Ok::<(), iqdb_types::IqdbError>(())
    /// ```
    #[must_use]
    pub fn metric(&self) -> DistanceMetric {
        self.metric
    }

    /// The number of searchable vectors in the index.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use iqdb_flat::{FlatConfig, FlatIndex};
    /// use iqdb_index::{Index, IndexCore};
    /// use iqdb_types::{DistanceMetric, VectorId};
    ///
    /// let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig)?;
    /// assert_eq!(idx.len(), 0);
    /// idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0][..]), None)?;
    /// assert_eq!(idx.len(), 1);
    /// # Ok::<(), iqdb_types::IqdbError>(())
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Returns `true` when the index holds no vectors.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_flat::{FlatConfig, FlatIndex};
    /// use iqdb_index::Index;
    /// use iqdb_types::DistanceMetric;
    ///
    /// let idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig)?;
    /// assert!(idx.is_empty());
    /// # Ok::<(), iqdb_types::IqdbError>(())
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn check_dim(&self, vector_len: usize) -> Result<()> {
        if vector_len != self.dim {
            return Err(IqdbError::DimensionMismatch {
                expected: self.dim,
                found: vector_len,
            });
        }
        Ok(())
    }

    /// Approximate resident footprint of the index, in bytes.
    ///
    /// Counts the row payload once. The value feeds
    /// [`IndexStats::memory_bytes`], which is documented as best-effort for
    /// capacity dashboards, not exact accounting.
    fn approximate_memory_bytes(&self) -> usize {
        // `Arc<[f32]>` allocates exactly `len * size_of::<f32>()` of
        // payload (no spare capacity) plus a fixed header for the strong
        // and weak refcounts. The header is two `usize`s — the documented
        // `ArcInner` layout for sized slices — independent of the payload
        // length.
        let arc_header_bytes = 2 * size_of::<usize>();
        let vectors_bytes = self
            .vectors
            .iter()
            .map(|arc| arc.len() * size_of::<f32>() + arc_header_bytes)
            .sum::<usize>()
            + self.vectors.capacity() * size_of::<Arc<[f32]>>();
        let ids_bytes = self.ids.capacity() * size_of::<VectorId>();
        let metadata_bytes = self.metadata.capacity() * size_of::<Option<Metadata>>();
        let seqs_bytes = self.seqs.capacity() * size_of::<u64>();
        // `HashMap` capacity overhead is implementation-defined; this is a
        // rough lower bound (key + value per slot) and matches the
        // "approximate" contract on `IndexStats::memory_bytes`.
        let id_to_pos_bytes =
            self.id_to_pos.capacity() * (size_of::<VectorId>() + size_of::<usize>());
        vectors_bytes + ids_bytes + metadata_bytes + seqs_bytes + id_to_pos_bytes
    }

    /// Filter-None search hot path.
    ///
    /// The candidate slice references are the only N-sized allocation
    /// this path makes beyond the distance buffer and the returned hits;
    /// `seqs` is read directly from `self.seqs[..]` and the top-`k`
    /// `chosen` indices are *storage* indices, mapped straight back into
    /// `self.ids` and `self.metadata`. No `accepted: Vec<usize>` /
    /// `accepted_seqs: Vec<u64>` indirection is built (that is the
    /// filter-Some path's cost). This keeps a no-filter search's
    /// allocation count independent of corpus size — see
    /// `tests/no_alloc.rs`.
    fn search_unfiltered(&self, query: &[f32], k: usize) -> Result<Vec<Hit>> {
        // Reborrow each `Arc<[f32]>` as `&[f32]` for the distance kernel.
        let candidates: Vec<&[f32]> = self.vectors.iter().map(|arc| &arc[..]).collect();
        let mut distances = vec![0.0_f32; candidates.len()];
        compute_distances(self.metric, query, &candidates, &mut distances)?;

        // DotProduct: raw value is "larger is more similar"; flip the
        // sign so `Hit.distance` follows the "smaller is nearer" contract
        // for every metric.
        if matches!(self.metric, DistanceMetric::DotProduct) {
            for value in distances.iter_mut() {
                *value = -*value;
            }
        }

        let chosen = topk::select_topk_indices(&distances, &self.seqs, k);
        let mut hits = Vec::with_capacity(chosen.len());
        for storage_idx in chosen {
            hits.push(Hit {
                id: self.ids[storage_idx].clone(),
                distance: distances[storage_idx],
                metadata: self.metadata[storage_idx].clone(),
            });
        }
        Ok(hits)
    }

    /// Filter-Some search path. Validates the filter once via
    /// [`FilterEvaluator::new`] (enforces depth and `In` caps;
    /// pathological filters surface as [`IqdbError::InvalidFilter`]),
    /// then collects surviving storage indices, the parallel `seqs`
    /// slice the tie-breaker needs, and the candidate slice references
    /// the per-row distance loop consumes.
    fn search_filtered(&self, query: &[f32], k: usize, filter: &Filter) -> Result<Vec<Hit>> {
        let evaluator = FilterEvaluator::new(filter.clone())?;
        let accepted: Vec<usize> = (0..self.ids.len())
            .filter(|&i| evaluator.evaluate(self.metadata[i].as_ref()))
            .collect();
        if accepted.is_empty() {
            return Ok(Vec::new());
        }

        let candidates: Vec<&[f32]> = accepted.iter().map(|&i| &self.vectors[i][..]).collect();
        let accepted_seqs: Vec<u64> = accepted.iter().map(|&i| self.seqs[i]).collect();
        let mut distances = vec![0.0_f32; candidates.len()];
        compute_distances(self.metric, query, &candidates, &mut distances)?;

        if matches!(self.metric, DistanceMetric::DotProduct) {
            for value in distances.iter_mut() {
                *value = -*value;
            }
        }

        let chosen = topk::select_topk_indices(&distances, &accepted_seqs, k);
        let mut hits = Vec::with_capacity(chosen.len());
        // INVARIANT: `chosen[i]` is a *candidate-space* index — i.e. a
        // position into `distances` and `accepted_seqs`, NOT a storage
        // position. `search_unfiltered` works directly in storage space
        // (no `accepted` indirection); the two paths intentionally differ
        // here. Changing `topk::select_topk_indices`'s index space would
        // silently produce wrong distances unless both call sites update.
        for candidate_idx in chosen {
            let storage_idx = accepted[candidate_idx];
            hits.push(Hit {
                id: self.ids[storage_idx].clone(),
                distance: distances[candidate_idx],
                metadata: self.metadata[storage_idx].clone(),
            });
        }
        Ok(hits)
    }
}

impl IndexCore for FlatIndex {
    fn insert(
        &mut self,
        id: VectorId,
        vector: Arc<[f32]>,
        metadata: Option<Metadata>,
    ) -> Result<()> {
        self.check_dim(vector.len())?;
        // Duplicate check is O(1) via the id→position map. The dimension
        // check fires before this so a bad-dim insert with an already-known
        // id still surfaces `DimensionMismatch`, not `Duplicate`.
        if self.id_to_pos.contains_key(&id) {
            return Err(IqdbError::Duplicate);
        }
        let pos = self.ids.len();
        let seq = self.next_seq;
        self.next_seq = self
            .next_seq
            .checked_add(1)
            .ok_or(IqdbError::InvalidConfig {
                reason: "FlatIndex insertion sequence counter overflowed u64",
            })?;
        // Take ownership of the caller's `Arc<[f32]>` directly — no payload
        // copy. A caller that keeps its own strong reference shares the
        // underlying `[f32]` allocation one-to-one with the index.
        self.vectors.push(vector);
        self.ids.push(id.clone());
        self.metadata.push(metadata);
        self.seqs.push(seq);
        let _prev = self.id_to_pos.insert(id, pos);
        Ok(())
    }

    /// Reserves capacity for all backing stores up front, then inserts each
    /// item via [`insert`](IndexCore::insert).
    ///
    /// This is the same fail-fast contract as the trait default (the first
    /// error returns immediately; inserts before it remain), but a single
    /// `reserve(items.len())` on each of the four `Vec`s and the
    /// `HashMap` replaces the `O(log n)` incremental reallocations the
    /// default loop would trigger — a measurable win for bulk loads.
    fn insert_batch(&mut self, items: Vec<(VectorId, Arc<[f32]>, Option<Metadata>)>) -> Result<()> {
        let additional = items.len();
        self.vectors.reserve(additional);
        self.ids.reserve(additional);
        self.metadata.reserve(additional);
        self.seqs.reserve(additional);
        self.id_to_pos.reserve(additional);
        for (id, vector, metadata) in items {
            self.insert(id, vector, metadata)?;
        }
        Ok(())
    }

    fn delete(&mut self, id: &VectorId) -> Result<()> {
        let pos = self.id_to_pos.remove(id).ok_or(IqdbError::NotFound)?;
        // swap_remove: O(1) per Vec; the row formerly at the back is moved
        // to `pos`. If that swap actually moved a row (i.e. `pos` wasn't
        // already the back), patch the swapped-in row's `id_to_pos` slot so
        // it still points at its new position. The `seq` it carries is
        // preserved — that is how the tiebreaker survives reordering.
        let _v = self.vectors.swap_remove(pos);
        let _i = self.ids.swap_remove(pos);
        let _m = self.metadata.swap_remove(pos);
        let _s = self.seqs.swap_remove(pos);
        if pos < self.ids.len() {
            let _prev = self.id_to_pos.insert(self.ids[pos].clone(), pos);
        }
        Ok(())
    }

    /// Searches for the top-`k` nearest neighbours under `params.metric`.
    ///
    /// Returns [`IqdbError::DimensionMismatch`] if `query.len() != self.dim`,
    /// [`IqdbError::InvalidMetric`] if the params metric does not match the
    /// index's, and [`IqdbError::InvalidFilter`] if `params.filter` is
    /// supplied and rejected by [`FilterEvaluator::new`] (depth or `In`
    /// cardinality past `iqdb_filter`'s `MAX_FILTER_DEPTH` /
    /// `MAX_IN_VALUES`). A pathological filter surfaces as a clean error
    /// rather than overflowing the search thread.
    fn search(&self, query: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
        self.check_dim(query.len())?;
        if params.metric != self.metric {
            return Err(IqdbError::InvalidMetric);
        }
        if params.k == 0 || self.ids.is_empty() {
            return Ok(Vec::new());
        }

        // Two paths. The filter-None hot path skips the `accepted` /
        // `accepted_seqs` materialisations the filter-Some path needs to
        // map back through — two N-sized allocations gone per search when
        // no filter is supplied (the common case). The filter-Some path is
        // unchanged because every allocation there carries information the
        // post-distance stage cannot recover.
        match &params.filter {
            None => self.search_unfiltered(query, params.k),
            Some(filter) => self.search_filtered(query, params.k, filter),
        }
    }

    fn len(&self) -> usize {
        FlatIndex::len(self)
    }

    fn is_empty(&self) -> bool {
        FlatIndex::is_empty(self)
    }

    fn dim(&self) -> usize {
        FlatIndex::dim(self)
    }

    fn metric(&self) -> DistanceMetric {
        FlatIndex::metric(self)
    }

    fn flush(&mut self) -> Result<()> {
        // Flat is purely in-memory; nothing to flush.
        Ok(())
    }

    fn stats(&self) -> IndexStats {
        IndexStats {
            n_vectors: self.ids.len(),
            memory_bytes: self.approximate_memory_bytes(),
            disk_bytes: None,
            index_type: "flat",
            // FlatIndex has no per-kind counters; reporting `None` avoids
            // allocating an empty HashMap on every `stats()` call.
            extra: None,
        }
    }
}

impl Index for FlatIndex {
    type Config = FlatConfig;

    fn new(dim: usize, metric: DistanceMetric, _config: Self::Config) -> Result<Self> {
        Self::new_unconfigured(dim, metric)
    }
}

fn compute_distances(
    metric: DistanceMetric,
    query: &[f32],
    candidates: &[&[f32]],
    out: &mut [f32],
) -> Result<()> {
    #[cfg(feature = "parallel")]
    {
        crate::parallel::compute_distances(metric, query, candidates, out)
    }
    #[cfg(not(feature = "parallel"))]
    {
        iqdb_distance::compute_batch(metric, query, candidates, out)
    }
}

#[cfg(test)]
mod tests {
    //! Pointer-identity proof for the zero-copy insert contract.
    //!
    //! [`IndexCore::insert`] takes the caller's `Arc<[f32]>` by value;
    //! [`FlatIndex`] stores it verbatim without allocating a fresh payload.
    //! A consumer uses this to share one underlying allocation between its
    //! record store and the index row.

    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn insert_stores_caller_arc_without_reallocating_payload() {
        let mut idx = FlatIndex::new_unconfigured(3, DistanceMetric::Euclidean).unwrap();
        let payload: Arc<[f32]> = Arc::from(&[1.0_f32, 2.0, 3.0][..]);
        let caller_ptr = Arc::as_ptr(&payload);

        idx.insert(VectorId::from(1u64), Arc::clone(&payload), None)
            .unwrap();

        let stored = &idx.vectors[0];
        assert_eq!(
            Arc::as_ptr(stored),
            caller_ptr,
            "FlatIndex MUST store the caller's Arc verbatim — no fresh \
             allocation, no copy",
        );
        // Caller + index = 2 strong refs.
        assert_eq!(Arc::strong_count(&payload), 2);
    }

    #[test]
    fn delete_drops_the_stored_strong_ref() {
        let mut idx = FlatIndex::new_unconfigured(2, DistanceMetric::Cosine).unwrap();
        let payload: Arc<[f32]> = Arc::from(&[0.5_f32, -0.5][..]);
        idx.insert(VectorId::from(9u64), Arc::clone(&payload), None)
            .unwrap();
        assert_eq!(Arc::strong_count(&payload), 2);

        idx.delete(&VectorId::from(9u64)).unwrap();
        assert_eq!(
            Arc::strong_count(&payload),
            1,
            "delete drops the index's strong ref; only the caller's remains",
        );
    }
}
