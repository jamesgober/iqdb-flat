//! Criterion benches for `FlatIndex` hot paths.
//!
//! Covers the three things that dominate flat's cost: the unfiltered search
//! scan, the metadata-filtered scan, and bulk insertion. Data is deterministic
//! (seeded from index + dim with `sin`/`cos`) so a second run reproduces the
//! baseline. Under `--features parallel` the search benches exercise the rayon
//! path automatically.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, Metadata, SearchParams, Value, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn row(i: usize, dim: usize) -> Vec<f32> {
    (0..dim).map(|j| ((i + j * 7) as f32).sin()).collect()
}

fn build_index(n: usize, dim: usize, metric: DistanceMetric) -> FlatIndex {
    let mut idx = FlatIndex::new(dim, metric, FlatConfig).expect("valid dim");
    for i in 0..n {
        idx.insert(VectorId::from(i as u64), arc(&row(i, dim)), None)
            .expect("fresh id");
    }
    idx
}

/// Like `build_index`, but tags every fifth row `tier = "hot"` so the filtered
/// bench has a ~20%-selective predicate to push down.
fn build_index_with_meta(n: usize, dim: usize, metric: DistanceMetric) -> FlatIndex {
    let mut idx = FlatIndex::new(dim, metric, FlatConfig).expect("valid dim");
    for i in 0..n {
        let tier = if i % 5 == 0 { "hot" } else { "cold" };
        let meta: Metadata = [("tier".to_string(), Value::String(tier.into()))]
            .into_iter()
            .collect();
        idx.insert(VectorId::from(i as u64), arc(&row(i, dim)), Some(meta))
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

fn bench_filtered_search(c: &mut Criterion) {
    let (n, dim, metric) = (10_000_usize, 128_usize, DistanceMetric::Euclidean);
    let idx = build_index_with_meta(n, dim, metric);
    let query = query_vector(dim);
    let params = SearchParams {
        filter: Some(Filter::eq("tier", Value::String("hot".into()))),
        ..SearchParams::new(10, metric)
    };
    let name = format!("flat/search_filtered/{metric:?}/n{n}/d{dim}/sel20/k10");
    let _ = c.bench_function(&name, |bencher| {
        bencher.iter(|| idx.search(black_box(&query), black_box(&params)));
    });
}

fn bench_insert_batch(c: &mut Criterion) {
    let (n, dim) = (10_000_usize, 128_usize);
    let metric = DistanceMetric::Euclidean;
    let items: Vec<_> = (0..n)
        .map(|i| (VectorId::from(i as u64), arc(&row(i, dim)), None))
        .collect();
    let name = format!("flat/insert_batch/n{n}/d{dim}");
    let _ = c.bench_function(&name, |bencher| {
        bencher.iter_batched(
            || items.clone(),
            |batch| {
                let mut idx = FlatIndex::new(dim, metric, FlatConfig).expect("valid dim");
                idx.insert_batch(black_box(batch)).expect("all valid");
                idx
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_search,
    bench_filtered_search,
    bench_insert_batch
);
criterion_main!(benches);
