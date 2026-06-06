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

## v0.5.0 -- large-scan correctness + API freeze

Exit criteria:
- [ ] Public API frozen (recorded here). `cargo audit` + `cargo deny` clean.

---

## v0.6.0 -> v0.9.x -- Alpha / Beta -> RC

- 0.6.x-0.7.x: integrate against real consumers; MINOR-compatible additions only.
- 0.8.x (beta): bug fixes; broader testing; final benchmarks.
- 0.9.x (rc): critical fixes + doc polish.

---

## v1.0.0 -- Stable

- [ ] Definition of Done (DIRECTIVES section 7) satisfied.
- [ ] Public API frozen until 2.0.
- [ ] Release note written; published to crates.io; tag pushed.

---

## Out of scope for 1.0

- Approximate search -- that is hnsw/ivf.
- Persistence/caching -- separate crates wrap this.
