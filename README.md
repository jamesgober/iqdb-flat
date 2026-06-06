<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>iqdb-flat</b>
    <br>
    <sub><sup>iQDB FLAT (EXACT) INDEX</sup></sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/iqdb-flat"><img alt="Crates.io" src="https://img.shields.io/crates/v/iqdb-flat"></a>
    <a href="https://crates.io/crates/iqdb-flat"><img alt="Downloads" src="https://img.shields.io/crates/d/iqdb-flat?color=%230099ff"></a>
    <a href="https://docs.rs/iqdb-flat"><img alt="docs.rs" src="https://img.shields.io/docsrs/iqdb-flat"></a>
    <a href="https://github.com/jamesgober/iqdb-flat/actions"><img alt="CI" src="https://github.com/jamesgober/iqdb-flat/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.87%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>iqdb-flat</strong> is brute-force exact nearest-neighbor search: the simplest possible index and the ground truth for every recall benchmark. It is built to be fast enough to be a real choice for small datasets, not just a reference.
    </p>
    <p>
        It is the first and simplest consumer of the `Index` trait, so it doubles as the trait's design validation.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.87+</strong> (Rust 2024 edition). Exact, correct, and the recall ground truth. Fast for small data.
    </p>
    <blockquote>
        <strong>Status: pre-1.0, feature-complete.</strong> The full index — exact search, top-<code>k</code>, the <code>Index</code> trait, optional parallel scans, and metadata pre-filtering — is implemented and tested. The public API is being finalised across the 0.x series and frozen at <code>1.0.0</code>. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

<h2>What it does</h2>

- **Exact search** &mdash; scans every vector, computes the true distance, returns the top-`k`; always correct, never approximate
- **Ground truth** &mdash; the reference every approximate index (HNSW, IVF) is measured against via recall@k
- **Deterministic** &mdash; one *smaller-is-nearer* ordering across all five metrics, with a stable insertion-order tiebreaker and NaN-safe selection
- **Fast where it counts** &mdash; SIMD distance via `iqdb-distance`, a bounded-heap `O(n log k)` top-`k`, amortized `O(1)` insert/delete, and an optional rayon parallel scan
- **Right for small data** &mdash; the obvious choice under ~10k vectors, where graph or partition overhead is not justified

<br>

## Installation

```toml
[dependencies]
iqdb-flat = "0.4"

# Optional rayon-backed parallel scan for large in-memory corpora:
# iqdb-flat = { version = "0.4", features = ["parallel"] }
```

<br>

## Quick Start

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

Filtered search restricts the scan to rows whose metadata matches, evaluated
before distance work so a selective filter skips proportionally:

```rust
# use iqdb_flat::{FlatConfig, FlatIndex};
# use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, SearchParams, Value};

# fn demo(idx: &FlatIndex) -> iqdb_types::Result<()> {
let params = SearchParams {
    filter: Some(Filter::eq("lang", Value::String("rust".into()))),
    ..SearchParams::new(10, DistanceMetric::Euclidean)
};
let hits = idx.search(&[0.0, 0.0], &params)?;
# Ok(())
# }
```

The complete surface — every method, parameter, error, and more examples — is in
[`docs/API.md`](./docs/API.md).

<br>

## Performance

- **Distance** is delegated to [`iqdb-distance`](https://crates.io/crates/iqdb-distance), which dispatches to SIMD kernels (AVX2/NEON) where the target allows; flat never reimplements a metric.
- **Top-`k`** uses a bounded max-heap of size `k` keyed by `(distance, sequence)` — `O(n log k)`, NaN-safe via `f32::total_cmp`.
- **Insert / delete** are amortized `O(1)`: a `HashMap` id→position map for duplicate checks, `swap_remove` for deletion, and a monotonic sequence stamp so reordering never disturbs the tiebreaker.
- **No-filter search** allocates a fixed number of buffers independent of corpus size (locked in by `tests/no_alloc.rs`).
- **`parallel` feature** adds a rayon chunked scan that is **byte-identical** to the sequential baseline (`tests/parallel_equivalence.rs`); small corpora short-circuit to sequential.

<br>

## Status

`v0.4.0` is **feature-complete**: exact search, top-`k`, the full `Index` /
`IndexCore` trait implementation, the optional `parallel` scan, and metadata
pre-filtering all ship and are covered by unit, property, differential, and
scale tests. Remaining work to 1.0 is API finalisation, polish, and final
benchmarks per the <a href="./dev/ROADMAP.md"><code>ROADMAP</code></a>.

<hr>
<br>

## Where It Fits

`iqdb-flat` is a Phase-3 index. It builds on:

- `iqdb-types` &mdash; vectors, ids, metadata
- `iqdb-distance` &mdash; the distance kernels
- `iqdb-index` &mdash; implements the `Index` trait
- `iqdb-eval` &mdash; uses it to generate ground truth

It is unblocked once `iqdb-index` exists; no external dependency.

<br>

## Contributing

See <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for engineering standards and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

<br>

<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> &mdash; <a href="./LICENSE-APACHE">LICENSE-APACHE</a></li>
        <li><b>MIT License</b> &mdash; <a href="./LICENSE-MIT">LICENSE-MIT</a></li>
    </ul>
    <p>at your option.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
