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
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
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
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). Exact, correct, and the recall ground truth. Fast for small data.
    </p>
    <blockquote>
        <strong>Status: pre-1.0, in active development.</strong> The public API is being designed across the 0.x series and frozen at <code>1.0.0</code>. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

<h2>What it does</h2>

- **Exact search** &mdash; scan every vector, compute distance, return top-k; always correct
- **Ground truth** &mdash; the reference approximate indexes are measured against
- **Fast where it can be** &mdash; batched distance, optional rayon parallel scans, SIMD via iqdb-distance
- **Right for small data** &mdash; the obvious choice under ~10k vectors where graph overhead is not justified


<br>

## Installation

```toml
[dependencies]
iqdb-flat = "0.1"
```

<br>

## Status

This is the <code>v0.1.0</code> scaffold: structure, tooling, and quality gates are in place; the implementation lands across the 0.x series per the <a href="./dev/ROADMAP.md"><code>ROADMAP</code></a> and <a href="./docs/API.md"><code>docs/API.md</code></a>.

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
