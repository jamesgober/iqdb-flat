//! Criterion benches for `FlatIndex::search`.
//!
//! Measures search latency at representative `(n, dim)` combinations under
//! Cosine and Euclidean. The data is deterministic (seeded from index +
//! dim with `sin`/`cos`) so a second run produces the same baseline.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn build_index(n: usize, dim: usize, metric: DistanceMetric) -> FlatIndex {
    let mut idx = FlatIndex::new(dim, metric, FlatConfig).expect("valid dim");
    for i in 0..n {
        let mut row = vec![0.0_f32; dim];
        for (j, slot) in row.iter_mut().enumerate() {
            *slot = ((i + j * 7) as f32).sin();
        }
        idx.insert(VectorId::from(i as u64), arc(&row), None)
            .expect("fresh id");
    }
    idx
}

fn query_vector(dim: usize) -> Vec<f32> {
    (0..dim).map(|j| (j as f32).cos()).collect()
}

fn bench_search(c: &mut Criterion) {
    let configs = [
        (1_000_usize, 128_usize, DistanceMetric::Cosine),
        (10_000_usize, 128_usize, DistanceMetric::Cosine),
        (10_000_usize, 768_usize, DistanceMetric::Cosine),
        (1_000_usize, 128_usize, DistanceMetric::Euclidean),
        (10_000_usize, 128_usize, DistanceMetric::Euclidean),
    ];
    for (n, dim, metric) in configs {
        let idx = build_index(n, dim, metric);
        let query = query_vector(dim);
        let params = SearchParams::new(10, metric);
        let name = format!("flat/search/{metric:?}/n{n}/d{dim}/k10");
        let _ = c.bench_function(&name, |bencher| {
            bencher.iter(|| idx.search(black_box(&query), black_box(&params)));
        });
    }
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
