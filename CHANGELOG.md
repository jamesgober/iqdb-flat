# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

---

## [0.5.0] - 2026-06-06

Large-scan correctness and the **public API freeze**. No new surface — this
release pins the oracle guarantee at scale and commits the API for the 1.x
series. Only additive, non-breaking changes are made from here to `1.0.0`.

### Added

- **Large-scan exact-correctness test** (`tests/large_scan.rs`) — asserts flat's
  top-`k` is bit-for-bit identical to an independent naive full scan at
  N = 20_000 (exact, via integer coordinates that avoid `f32` roundoff), and that
  a full `k == N` scan returns every id exactly once in best-first order.

### Changed

- **Public API frozen.** The committed surface is recorded in `dev/ROADMAP.md`
  (§ v0.5.0). `cargo audit` and `cargo deny check` are clean.

---

## [0.4.0] - 2026-06-06

The flat index is **feature-complete**. This release lands the implementation
on top of the v0.1.0 scaffold: exact search, the `Index` trait, the optional
parallel scan, and metadata pre-filtering — wired to the stable (`1.0`) iQDB
spine crates. The public API is finalised across the remaining 0.x series and
frozen at `1.0.0`.

### Added

- **`FlatIndex`** — brute-force exact nearest-neighbour index implementing
  `iqdb_index::IndexCore` and `iqdb_index::Index` (`type Config = FlatConfig`).
  Insert, delete, batch insert, single and batch search, `flush`, and `stats`.
- **`FlatConfig`** — unit configuration type satisfying the `Index::Config`
  (`Default + Clone`) bound; a seam for future knobs without an API break.
- **Exact top-`k` search** — a bounded max-heap selector (`O(n log k)`) keyed by
  `(distance, insertion-sequence)` via `f32::total_cmp`: NaN-safe and
  deterministic.
- **One ordering invariant** — `Hit.distance` is *smaller-is-nearer* for all five
  metrics; `DotProduct` is negated at the boundary.
- **Stable tiebreaker** — equal-distance hits are ranked by insertion order,
  preserved across `swap_remove` deletes and delete-then-reinsert sequences via
  monotonic per-row sequence stamps.
- **Amortized `O(1)` insert/delete** — a `HashMap` id→position map for duplicate
  detection and `swap_remove` deletion, independent of corpus size.
- **Zero-copy insert** — the caller's `Arc<[f32]>` payload is stored verbatim; no
  fresh `[f32]` allocation, so a consumer can share one allocation with the index.
- **Metadata pre-filtering** — `SearchParams::filter` is evaluated through
  `iqdb-filter` before distance computation, so a selective filter skips work
  proportionally.
- **Optional `parallel` feature** — a rayon-backed chunked distance scan that is
  byte-identical to the sequential baseline; small corpora short-circuit to
  sequential.
- **`VERSION`** — the crate's compile-time SemVer string.
- **Tests** — unit, `proptest` invariants, an independent differential oracle for
  all five metrics, parallel-vs-sequential equivalence, a no-filter allocation
  invariant, tiebreaker/determinism suites, and a 50k-vector scale test.
- **Benchmarks** — `criterion` search benches across representative `(n, dim)`
  combinations under Cosine and Euclidean.
- **`docs/API.md`** — complete reference for the public surface, with examples.

### Changed

- Wired dependencies to the stable iQDB spine: `iqdb-types`, `iqdb-distance`,
  `iqdb-index`, and `iqdb-filter` (all `1.0`).
- Added Matt Callahan to the crate authors.

---

## [0.1.0] - 2026-05-30

Initial scaffold and repository bootstrap. No domain logic yet &mdash; this release establishes the structure, tooling, and quality gates the implementation will be built on.

### Added

- `Cargo.toml` with crate metadata, Rust 2024 edition, MSRV 1.87.
- Dual `Apache-2.0 OR MIT` license files.
- `README.md`, `CHANGELOG.md`, and a documentation skeleton.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml` CI matrix; `deny.toml`, `clippy.toml`, `rustfmt.toml`.
- `dev/DIRECTIVES.md` and `dev/ROADMAP.md` (committed engineering standards + plan).

[Unreleased]: https://github.com/jamesgober/iqdb-flat/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/jamesgober/iqdb-flat/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/iqdb-flat/compare/v0.1.0...v0.4.0
[0.1.0]: https://github.com/jamesgober/iqdb-flat/releases/tag/v0.1.0
