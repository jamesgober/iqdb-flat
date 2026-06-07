# iqdb-flat -- Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.

---

## v0.1.0 -- Scaffold (DONE)

Compiles, CI green, structure correct, no domain logic.

- [x] Manifest, README, CHANGELOG, REPS, license, CI, lints in place.
- [x] API surface sketched in `docs/API.md`.

---

## v0.2.0 -- exact search + top-k + `Index` impl (THE HARD PART, NOT DEFERRED) (DONE)

Exit criteria:
- [x] Every public item has rustdoc + a runnable example.
- [x] Core invariants property-tested.

Landed together with 0.3/0.4 in the consolidated v0.4.0 release (see CHANGELOG).

---

## v0.3.0 -- rayon parallel scans + contiguous memory layout (PARALLEL DONE; LAYOUT DEFERRED)

Exit criteria:
- [x] New surface tested and benchmarked where it is a hot path.

The rayon parallel scan shipped in v0.4.0, proven byte-identical to the
sequential baseline (`tests/parallel_equivalence.rs`) and benchmarked.

**Deferral (recorded per the anti-deferral rule).** The *contiguous row-major
memory layout* is deferred to a post-0.4 internal optimisation. Rationale: the
current `Vec<Arc<[f32]>>` row layout is a deliberate zero-copy design — `insert`
stores the caller's `Arc` verbatim so a consumer shares one payload allocation
with the index. A flat `Vec<f32>` of `n * dim` would trade that zero-copy insert
away. The layout is purely internal and does not affect the public API, so it
does not block the 0.5 API freeze or 1.0; it can land in any later 0.x/1.x as a
non-breaking change once benchmarked against a real consumer's access pattern.

---

## v0.4.0 -- metadata pre-filter integration + feature freeze (DONE)

Exit criteria:
- [x] No `todo!`/`unimplemented!`. Feature freeze declared.

Metadata pre-filtering via `iqdb-filter` (evaluated before distance work) shipped
in v0.4.0. **Feature freeze is declared:** the feature set — exact search,
top-`k`, the `Index`/`IndexCore` impl, the optional `parallel` scan, and metadata
pre-filtering — is complete. Only the internal layout optimisation above remains,
and it is not a feature-surface change.

---

## v0.5.0 -- large-scan correctness + API freeze (DONE)

Exit criteria:
- [x] Public API frozen (recorded here). `cargo audit` + `cargo deny` clean.

**Large-scan correctness.** `tests/large_scan.rs` asserts flat's top-`k` is
bit-for-bit identical to an independent naive full scan at N = 20_000 (exact, via
integer coordinates that avoid `f32` roundoff), and that a full `k == N` scan
returns every id exactly once in best-first order — pinning the oracle guarantee
at scale, not just in the small unit tests.

### Frozen public API (1.x compatibility surface)

Recorded here per the directive that the public surface is frozen at the API
freeze. Everything below is committed; only **additive, non-breaking** changes
are made through 1.x. `iqdb_types::IqdbError` (the error type all methods return)
is `#[non_exhaustive]`, so new variants are not breaking.

- **`iqdb_flat::VERSION: &str`** — compile-time SemVer string.
- **`iqdb_flat::Hit`** — re-export of `iqdb_types::Hit` (the search result type).
- **`FlatConfig`** — unit config; `derive(Debug, Default, Clone, PartialEq, Eq)`.
- **`FlatIndex`** — `derive(Debug)`; `Send + Sync`. Inherent methods:
  - `FlatIndex::new_unconfigured(dim, metric) -> Result<Self>`
  - `dim(&self) -> usize`, `metric(&self) -> DistanceMetric`,
    `len(&self) -> usize`, `is_empty(&self) -> bool`
- **`impl iqdb_index::Index for FlatIndex`** — `type Config = FlatConfig`;
  `new(dim, metric, config) -> Result<Self>`.
- **`impl iqdb_index::IndexCore for FlatIndex`** — `insert`, `insert_batch`*,
  `delete`, `search`, `search_batch`*, `len`, `is_empty`, `dim`, `metric`,
  `flush`, `stats` (* = trait default, not overridden).
- **Feature `parallel`** — internal-only; does not change the API surface.

Behavioural contracts frozen with the surface: `Hit.distance` is
smaller-is-nearer for all five metrics (`DotProduct` negated); equal-distance
ties break by insertion order via monotonic per-row sequence stamps; no method
panics on any input (`NaN`/`±∞` sort deterministically via `f32::total_cmp`);
zero `unsafe`.

---

## v0.6.0 -> v0.9.x -- Alpha / Beta -> RC

- **0.6.0 (DONE)** — runnable `examples/` suite shipped (`quick_start`,
  `metric_tour`, `filtered_search`, `recall_oracle`); each asserts its output.
  API-frozen, additive only. Enters the alpha band.
- 0.6.x-0.7.x: integrate against real consumers; MINOR-compatible additions only.
- 0.8.x (beta): bug fixes; broader testing; final benchmarks.
- 0.9.x (rc): critical fixes + doc polish.

**Blocked-work note.** The substantive 0.7-0.9 work — exercising flat as the
recall oracle against `iqdb-hnsw` / `iqdb-ivf` and running cross-crate
benchmarks — depends on those consumer crates and `iqdb-eval`, which live in
separate repos. flat's own surface is complete and frozen; it does not change
during this band. `loom` is **not applicable** (no lock-free / shared-mutable
path; single-writer-internal), so the DoD's loom criterion is satisfied vacuously.

---

## v1.0.0 -- Stable

- [ ] Definition of Done (DIRECTIVES section 7) satisfied.
- [ ] Public API frozen until 2.0.
- [ ] Release note written; published to crates.io; tag pushed.

---

## Out of scope for 1.0

- Approximate search -- that is hnsw/ivf.
- Persistence/caching -- separate crates wrap this.
