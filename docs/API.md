# iqdb-flat &mdash; API Reference

> Complete reference for **every** public item in `iqdb-flat` as of
> **v1.0.0**: what it is, its parameters and return shape, the traits it
> implements, and worked examples for each use case.
>
> **Status: stable (1.0).** The public surface is committed under SemVer for the
> 1.x series — no breaking changes until 2.0 (the frozen surface is recorded in
> [`dev/ROADMAP.md`](../dev/ROADMAP.md)); only additive, non-breaking changes are
> made within 1.x.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Example pointers](#example-pointers)
- [Quick start](#quick-start)
- [Crate constants](#crate-constants)
  - [`VERSION`](#version)
- [`FlatConfig`](#flatconfig)
- [`FlatIndex`](#flatindex)
  - [Construction](#construction)
    - [`FlatIndex::new`](#flatindexnew)
    - [`FlatIndex::new_unconfigured`](#flatindexnew_unconfigured)
  - [Accessors](#accessors)
    - [`dim`](#flatindexdim) · [`metric`](#flatindexmetric) · [`len`](#flatindexlen) · [`is_empty`](#flatindexis_empty)
  - [Write operations](#write-operations)
    - [`insert`](#indexcoreinsert) · [`insert_batch`](#indexcoreinsert_batch) · [`delete`](#indexcoredelete) · [`flush`](#indexcoreflush)
  - [Read operations](#read-operations)
    - [`search`](#indexcoresearch) · [`search_batch`](#indexcoresearch_batch) · [`stats`](#indexcorestats)
- [Result ordering & the stable tiebreaker](#result-ordering--the-stable-tiebreaker)
- [Metadata filtering](#metadata-filtering)
- [Errors](#errors)
- [Feature flags](#feature-flags)
- [Trait implementation matrix](#trait-implementation-matrix)

---

## Overview

`iqdb-flat` is **brute-force exact** nearest-neighbour search. It scans every
stored vector on every query, computes the true distance, and returns the
top-`k`. There is no approximation and no recall loss — which makes it two
things at once:

- **The ground truth.** Every approximate index (HNSW, IVF) is validated
  against flat results via recall@k. If flat says a vector is in the true
  top-`k`, it is.
- **A real index for small data.** Under roughly 10k vectors, the constant-factor
  overhead of a graph or an inverted file is rarely worth it; a tight linear
  scan with SIMD distance kernels wins on both latency and simplicity.

It is also the first and simplest implementer of the
[`iqdb_index::Index`](#trait-implementation-matrix) trait, so it doubles as that
trait's design validation.

| Property | Guarantee |
|---|---|
| Recall | Exact — always the true top-`k`. |
| Ordering | `Hit.distance` is **smaller-is-nearer** for all five metrics. |
| Ties | Deterministic — earlier insertion wins (see [tiebreaker](#result-ordering--the-stable-tiebreaker)). |
| Insert / delete | Amortized `O(1)` each, independent of corpus size. |
| Search | `O(n · dim)` distance work + `O(n log k)` selection. |
| Panics | None on any input — every fallible call returns `iqdb_types::Result`. |
| `unsafe` | Zero (`#![forbid(unsafe_code)]`). |

All distance math is delegated to [`iqdb_distance`](https://crates.io/crates/iqdb-distance);
flat never reimplements a metric.

---

## Installation

```toml
[dependencies]
iqdb-flat = "1.0"

# Optional: rayon-backed parallel scan for large in-memory corpora.
# iqdb-flat = { version = "1.0", features = ["parallel"] }
```

---

## Example pointers

Every example under [`examples/`](../examples) is runnable and asserts its own
output (`cargo run --example <name>`):

- **`quick_start`** — shortest end-to-end build / insert / search / `stats`.
- **`metric_tour`** — one query under all five metrics; the smaller-is-nearer
  contract and the `DotProduct` negation.
- **`filtered_search`** — metadata pre-filtering with a compound `AND` / `>`
  filter and the closed-world rule.
- **`batch_and_stats`** — bulk loading via `insert_batch` (reserved capacity),
  reading `stats`, and fail-fast batch semantics.
- **`lifecycle`** — insert / delete / re-insert and the stable insertion-order
  tiebreaker across deletes.
- **`polymorphic`** — driving `FlatIndex` through `Box<dyn IndexCore>` (the
  engine's view).
- **`recall_oracle`** — measuring recall@k of a pruned stand-in against flat's
  exact top-`k` — flat's reason for existing.
- **`parallel_scan`** — the optional rayon scan (`--features parallel`).

---

## Quick start

```rust
use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

fn main() -> iqdb_types::Result<()> {
    // A 2-D Euclidean index. `FlatConfig` is a unit struct — nothing to tune.
    let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig)?;

    idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0][..]), None)?;
    idx.insert(VectorId::from(2u64), Arc::<[f32]>::from(&[3.0, 4.0][..]), None)?;
    idx.insert(VectorId::from(3u64), Arc::<[f32]>::from(&[1.0, 0.0][..]), None)?;

    let hits = idx.search(&[0.0, 0.0], &SearchParams::new(2, DistanceMetric::Euclidean))?;

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, VectorId::U64(1)); // distance 0.0
    assert_eq!(hits[1].id, VectorId::U64(3)); // distance 1.0
    Ok(())
}
```

---

## Crate constants

### `VERSION`

```rust
pub const VERSION: &str;
```

The crate's compile-time version (`CARGO_PKG_VERSION`), a `major.minor.patch`
SemVer core. Use it to report the exact `iqdb-flat` build a binary links
against — useful in diagnostics and version-skew checks across the iQDB crate
family.

```rust
let v = iqdb_flat::VERSION;
assert_eq!(v.split('.').count(), 3);
assert!(v.split('.').all(|part| !part.is_empty()));
```

---

## `FlatConfig`

```rust
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlatConfig;
```

The configuration type for [`FlatIndex`]. It is a **unit struct**: the flat
index has no tunable knobs — its behaviour is fully determined by the dimension
and metric passed to [`new`](#flatindexnew). `FlatConfig` exists so `FlatIndex`
satisfies [`iqdb_index::Index`]'s associated `Config: Default + Clone` bound, and
so any future knob (initial capacity, parallel chunk size) can be added here
without changing the trait surface.

```rust
use iqdb_flat::FlatConfig;

let config = FlatConfig;
let _cloned = config.clone(); // Clone + Default + Eq all derived
assert_eq!(FlatConfig::default(), config);
```

---

## `FlatIndex`

```rust
#[derive(Debug)]
pub struct FlatIndex { /* private */ }
```

The brute-force exact index. It owns its vectors as `Arc<[f32]>` rows, the
parallel ids, optional per-row metadata, monotonic insertion stamps, and an
`id → position` map that keeps duplicate-detection and deletion `O(1)`.

`FlatIndex` is `Send + Sync` and **single-writer-internal**: it requires no
internal locking. Many threads may call the `&self` read methods
([`search`](#indexcoresearch), [`len`](#flatindexlen), …) concurrently, while the
`&mut self` write methods ([`insert`](#indexcoreinsert),
[`delete`](#indexcoredelete)) require exclusive access — exactly the contract the
engine's per-shard `RwLock` provides.

### Construction

#### `FlatIndex::new`

```rust
fn new(dim: usize, metric: DistanceMetric, config: FlatConfig) -> Result<FlatIndex>;
```

The [`iqdb_index::Index::new`] construction surface — the idiomatic constructor.
Builds an empty index for `dim`-component vectors compared under `metric`.

- **`dim`** — vector dimensionality; must be `> 0`.
- **`metric`** — the distance metric all searches use; queries that pass a
  different metric are rejected at search time.
- **`config`** — a [`FlatConfig`] (unit struct).
- **Returns** `Ok(FlatIndex)`, or [`Err(IqdbError::InvalidConfig)`](#errors) if
  `dim == 0`.

```rust
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::Index;
use iqdb_types::DistanceMetric;

let idx = FlatIndex::new(768, DistanceMetric::Cosine, FlatConfig).expect("dim > 0");
let err = FlatIndex::new(0, DistanceMetric::Cosine, FlatConfig);
assert!(err.is_err()); // dim must be > 0
```

#### `FlatIndex::new_unconfigured`

```rust
pub fn new_unconfigured(dim: usize, metric: DistanceMetric) -> Result<FlatIndex>;
```

The same construction without naming [`FlatConfig`] — convenient when the
concrete type is known and there is nothing to configure. Equivalent to
`FlatIndex::new(dim, metric, FlatConfig)`.

- **`dim`** — vector dimensionality; must be `> 0`.
- **`metric`** — the distance metric.
- **Returns** `Ok(FlatIndex)`, or [`Err(IqdbError::InvalidConfig)`](#errors) if
  `dim == 0`.

```rust
use iqdb_flat::FlatIndex;
use iqdb_types::DistanceMetric;

let idx = FlatIndex::new_unconfigured(3, DistanceMetric::Euclidean).expect("dim > 0");
assert_eq!(idx.dim(), 3);
assert!(idx.is_empty());
```

### Accessors

All four are `&self`, `O(1)`, and never fail.

#### `FlatIndex::dim`

```rust
pub fn dim(&self) -> usize;
```

The dimensionality the index was built for. Also available through
[`IndexCore::dim`].

#### `FlatIndex::metric`

```rust
pub fn metric(&self) -> DistanceMetric;
```

The distance metric the index was built for. A [`search`](#indexcoresearch) whose
`SearchParams::metric` differs from this returns
[`IqdbError::InvalidMetric`](#errors).

#### `FlatIndex::len`

```rust
pub fn len(&self) -> usize;
```

The number of searchable vectors. Deleted ids are not counted.

#### `FlatIndex::is_empty`

```rust
pub fn is_empty(&self) -> bool;
```

`true` when the index holds no vectors.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, VectorId};

let mut idx = FlatIndex::new(2, DistanceMetric::Cosine, FlatConfig).expect("ok");
assert!(idx.is_empty());
assert_eq!(idx.metric(), DistanceMetric::Cosine);

idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[1.0, 0.0][..]), None).expect("ok");
assert_eq!(idx.len(), 1);
assert!(!idx.is_empty());
```

### Write operations

These come from the [`iqdb_index::IndexCore`] trait; bring it into scope with
`use iqdb_index::IndexCore;`.

#### `IndexCore::insert`

```rust
fn insert(&mut self, id: VectorId, vector: Arc<[f32]>, metadata: Option<Metadata>) -> Result<()>;
```

Insert one vector. **Zero-copy:** the index takes ownership of the caller's
`Arc<[f32]>` and stores it verbatim — it never allocates a fresh `[f32]` buffer.
A caller that keeps its own `Arc` clone shares one underlying allocation with
the index.

- **`id`** — the vector's identity. Must not already be present.
- **`vector`** — the payload; `vector.len()` must equal [`dim`](#flatindexdim).
- **`metadata`** — optional key/value map used by [filtered search](#metadata-filtering).
- **Returns** `Ok(())`, or:
  - [`DimensionMismatch`](#errors) if `vector.len() != dim` (checked first).
  - [`Duplicate`](#errors) if `id` is already present.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, IqdbError, VectorId};

let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig).expect("ok");
idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0][..]), None).expect("ok");

// Duplicate id is rejected.
let dup = idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[9.0, 9.0][..]), None);
assert_eq!(dup, Err(IqdbError::Duplicate));

// Wrong dimension is rejected.
let bad = idx.insert(VectorId::from(2u64), Arc::<[f32]>::from(&[0.0][..]), None);
assert!(matches!(bad, Err(IqdbError::DimensionMismatch { expected: 2, found: 1 })));
```

#### `IndexCore::insert_batch`

```rust
fn insert_batch(&mut self, items: Vec<(VectorId, Arc<[f32]>, Option<Metadata>)>) -> Result<()>;
```

Insert many vectors in one call. **Fail-fast:** the first error returns
immediately and inserts that already succeeded remain in the index (it is not
transactional). Flat **overrides** the trait default to `reserve` capacity in all
backing stores up front, so a bulk load avoids the `O(log n)` incremental
reallocations a naive per-item loop would trigger — same semantics, fewer
allocations.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, VectorId};

let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig).expect("ok");
idx.insert_batch(vec![
    (VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0][..]), None),
    (VectorId::from(2u64), Arc::<[f32]>::from(&[1.0, 1.0][..]), None),
]).expect("both valid");
assert_eq!(idx.len(), 2);
```

#### `IndexCore::delete`

```rust
fn delete(&mut self, id: &VectorId) -> Result<()>;
```

Remove the vector identified by `id` from the search space. After a successful
delete, `search` never returns `id` until it is re-inserted. Re-inserting a
deleted id is allowed and gives it a fresh insertion stamp (see the
[tiebreaker](#result-ordering--the-stable-tiebreaker)).

- **Returns** `Ok(())`, or [`NotFound`](#errors) if no searchable vector has that
  id.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, IqdbError, VectorId};

let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).expect("ok");
idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[1.0][..]), None).expect("ok");
idx.delete(&VectorId::from(1u64)).expect("present");
assert_eq!(idx.delete(&VectorId::from(1u64)), Err(IqdbError::NotFound));
```

#### `IndexCore::flush`

```rust
fn flush(&mut self) -> Result<()>;
```

Commit pending state to durable storage. `FlatIndex` is purely in-memory, so
this always returns `Ok(())` immediately. It exists for trait-object uniformity
with indexes that *do* persist.

### Read operations

#### `IndexCore::search`

```rust
fn search(&self, query: &[f32], params: &SearchParams) -> Result<Vec<Hit>>;
```

Run an exact top-`k` search. Returns up to `params.k` [`Hit`]s ordered
best-first (smallest distance first; ties broken by insertion order).

- **`query`** — the query vector; `query.len()` must equal [`dim`](#flatindexdim).
- **`params`** — [`SearchParams`]: `k`, the `metric` (must match the index's), and
  an optional [`filter`](#metadata-filtering).
- **Returns** `Ok(Vec<Hit>)` (possibly empty), or:
  - [`DimensionMismatch`](#errors) if `query.len() != dim`.
  - [`InvalidMetric`](#errors) if `params.metric != self.metric()`.
  - [`InvalidFilter`](#errors) if `params.filter` is supplied and malformed
    (exceeds `iqdb_filter`'s depth / `IN`-cardinality caps).

`k == 0`, an empty index, or a filter that matches nothing all return an empty
`Vec` (not an error).

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).expect("ok");
for (id, x) in [(1u64, 5.0_f32), (2, 1.0), (3, 3.0)] {
    idx.insert(VectorId::from(id), Arc::<[f32]>::from(&[x][..]), None).expect("ok");
}
let hits = idx.search(&[0.0], &SearchParams::new(2, DistanceMetric::Euclidean)).expect("ok");
assert_eq!(hits[0].id, VectorId::U64(2)); // nearest to 0.0
assert_eq!(hits[1].id, VectorId::U64(3));
```

**DotProduct note.** For [`DistanceMetric::DotProduct`] the raw inner product is a
*similarity* (larger is more similar); flat negates it so `Hit.distance` stays
smaller-is-nearer like every other metric. A hit at raw dot `10.0` reports
`distance == -10.0`.

#### `IndexCore::search_batch`

```rust
fn search_batch(&self, queries: &[&[f32]], params: &SearchParams) -> Result<Vec<Vec<Hit>>>;
```

Run several searches with shared `params`, preserving input order in the outer
`Vec`. Provided by the trait default (loops over [`search`](#indexcoresearch));
flat does not override it.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).expect("ok");
idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0][..]), None).expect("ok");
let q0 = [0.0_f32];
let q1 = [10.0_f32];
let queries: [&[f32]; 2] = [&q0, &q1];
let results = idx.search_batch(&queries, &SearchParams::new(1, DistanceMetric::Euclidean)).expect("ok");
assert_eq!(results.len(), 2);
```

#### `IndexCore::stats`

```rust
fn stats(&self) -> IndexStats;
```

A runtime snapshot. `FlatIndex` reports `n_vectors`, an approximate
`memory_bytes`, `disk_bytes == None` (in-memory), `index_type == "flat"`, and
`extra == None` (no per-kind counters).

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, VectorId};

let mut idx = FlatIndex::new(3, DistanceMetric::Euclidean, FlatConfig).expect("ok");
idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0, 0.0, 0.0][..]), None).expect("ok");
let s = idx.stats();
assert_eq!(s.n_vectors, 1);
assert_eq!(s.index_type, "flat");
assert_eq!(s.disk_bytes, None);
assert!(s.memory_bytes > 0);
```

---

## Result ordering & the stable tiebreaker

Two guarantees make flat's output deterministic and a usable oracle:

1. **Smaller is nearer.** `Hit.distance` is ordered so the closest vector sorts
   first, for every metric. DotProduct is negated at the boundary to honour this.
2. **Earlier insertion wins ties.** When two vectors have an identical distance,
   the one inserted first is ranked higher. This holds across feature flags and
   across delete-then-reinsert sequences: each row carries a monotonic insertion
   stamp, and a re-inserted id gets a *fresh* (higher) stamp, so it moves to the
   end of any tie. Selection uses `f32::total_cmp`, so `NaN` distances sort
   last deterministically rather than panicking.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).expect("ok");
// Three vectors all at distance 1.0 from the query.
for id in [10u64, 20, 30] {
    idx.insert(VectorId::from(id), Arc::<[f32]>::from(&[1.0][..]), None).expect("ok");
}
let hits = idx.search(&[0.0], &SearchParams::new(3, DistanceMetric::Euclidean)).expect("ok");
// Tie broken by insertion order.
assert_eq!(hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
           vec![VectorId::U64(10), VectorId::U64(20), VectorId::U64(30)]);
```

---

## Metadata filtering

Set `SearchParams::filter` to a [`iqdb_types::Filter`] to restrict the search to
rows whose metadata matches. The filter is evaluated **before** distance
computation, so a selective filter skips distance work in proportion to how much
it rejects. Filter semantics (closed-world: a row with no metadata matches no
positive predicate) and validation are owned by
[`iqdb-filter`](https://crates.io/crates/iqdb-filter); flat applies one shared
definition so every iQDB index filters identically.

```rust
use std::sync::Arc;
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, Metadata, SearchParams, Value, VectorId};

let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).expect("ok");
let meta: Metadata = [("lang".to_string(), Value::String("rust".into()))].into_iter().collect();
idx.insert(VectorId::from(1u64), Arc::<[f32]>::from(&[0.0][..]), Some(meta)).expect("ok");
idx.insert(VectorId::from(2u64), Arc::<[f32]>::from(&[0.0][..]), None).expect("ok");

let params = SearchParams {
    filter: Some(Filter::eq("lang", Value::String("rust".into()))),
    ..SearchParams::new(10, DistanceMetric::Euclidean)
};
let hits = idx.search(&[0.0], &params).expect("ok");
assert_eq!(hits.len(), 1);
assert_eq!(hits[0].id, VectorId::U64(1)); // id 2 has no metadata → filtered out
```

---

## Errors

`iqdb-flat` defines **no error type of its own** — every fallible method returns
`iqdb_types::Result<T>` (`Result<T, iqdb_types::IqdbError>`). `IqdbError` is
`#[non_exhaustive]` and built on `error-forge`. The variants flat returns:

| Variant | When |
|---|---|
| `InvalidConfig { reason }` | `dim == 0` at construction; or the insertion-sequence counter overflows `u64` (practically unreachable). |
| `DimensionMismatch { expected, found }` | An inserted vector or a query has the wrong length. |
| `Duplicate` | `insert` of an id already present. |
| `NotFound` | `delete` of an id not present. |
| `InvalidMetric` | `search` whose `params.metric` differs from the index's metric. |
| `InvalidFilter` | `search` with a malformed filter (past `iqdb_filter`'s depth / `IN`-cardinality caps). |

No method panics on any input, including non-finite (`NaN`/`±∞`) vector
components — those flow through `f32::total_cmp` and sort deterministically.

---

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `parallel` | off | Adds a rayon-backed chunked distance scan for large corpora. The sequential path is the correctness baseline; the parallel path is **byte-identical** to it (enforced by `tests/parallel_equivalence.rs`). Small corpora short-circuit to the sequential path regardless. |

The public API is identical with and without `parallel` — it changes only how
the distance scan is executed internally.

---

## Trait implementation matrix

| Trait | Source | Notes |
|---|---|---|
| [`IndexCore`] | `iqdb-index` | Full operational surface: `insert`, `insert_batch`†, `delete`, `search`, `search_batch`*, `len`, `is_empty`, `dim`, `metric`, `flush`, `stats`. (* = trait default; † = overridden to reserve capacity.) |
| [`Index`] | `iqdb-index` | Typed construction; `type Config = FlatConfig`. |
| `Debug` | derived | On `FlatIndex` and `FlatConfig`. |
| `Default`, `Clone`, `PartialEq`, `Eq` | derived | On `FlatConfig`. |
| `Send + Sync` | auto | `FlatIndex` is shareable; single-writer-internal (no internal locking). |

[`Hit`]: https://docs.rs/iqdb-types/latest/iqdb_types/struct.Hit.html
[`SearchParams`]: https://docs.rs/iqdb-types/latest/iqdb_types/struct.SearchParams.html
[`iqdb_types::Filter`]: https://docs.rs/iqdb-types/latest/iqdb_types/enum.Filter.html
[`DistanceMetric::DotProduct`]: https://docs.rs/iqdb-types/latest/iqdb_types/enum.DistanceMetric.html
[`IndexCore`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.IndexCore.html
[`Index`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.Index.html
[`iqdb_index::Index`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.Index.html
[`iqdb_index::Index::new`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.Index.html#tymethod.new
[`iqdb_index::IndexCore`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.IndexCore.html
[`IndexCore::dim`]: https://docs.rs/iqdb-index/latest/iqdb_index/trait.IndexCore.html#tymethod.dim
[`FlatConfig`]: #flatconfig
[`FlatIndex`]: #flatindex

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
