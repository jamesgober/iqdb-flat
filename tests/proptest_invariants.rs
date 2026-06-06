//! Property-based invariants for `FlatIndex::search`.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, Metadata, SearchParams, Value, VectorId};
use proptest::prelude::*;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn deterministic_row(seed: u64, dim: usize) -> Vec<f32> {
    (0..dim)
        .map(|j| ((seed.wrapping_mul(31).wrapping_add(j as u64 * 97)) as f32).sin() + 0.5)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn topk_is_sorted_best_first(
        n in 1usize..40,
        dim in 1usize..16,
        k in 0usize..50,
    ) {
        let metric = DistanceMetric::Euclidean;
        let mut idx = FlatIndex::new(dim, metric, FlatConfig).unwrap();
        for i in 0..n {
            let row = deterministic_row(i as u64, dim);
            idx.insert(VectorId::from(i as u64), arc(&row), None).unwrap();
        }
        let query = deterministic_row(99_999, dim);
        let hits = idx.search(&query, &SearchParams::new(k, metric)).unwrap();

        prop_assert!(hits.len() <= k);
        prop_assert!(hits.len() <= n);
        for window in hits.windows(2) {
            prop_assert!(window[0].distance.total_cmp(&window[1].distance) != std::cmp::Ordering::Greater);
        }
    }

    #[test]
    fn full_topk_returns_every_id_once(
        n in 1usize..30,
        dim in 1usize..8,
    ) {
        let metric = DistanceMetric::Manhattan;
        let mut idx = FlatIndex::new(dim, metric, FlatConfig).unwrap();
        for i in 0..n {
            let row = deterministic_row(i as u64 + 1, dim);
            idx.insert(VectorId::from(i as u64), arc(&row), None).unwrap();
        }
        let query = deterministic_row(7, dim);
        let hits = idx.search(&query, &SearchParams::new(n, metric)).unwrap();

        prop_assert_eq!(hits.len(), n);
        let mut ids: Vec<u64> = hits
            .iter()
            .map(|h| match &h.id {
                VectorId::U64(v) => *v,
                VectorId::Bytes(_) => panic!("test inserts only U64 ids"),
            })
            .collect();
        ids.sort_unstable();
        let expected: Vec<u64> = (0..n as u64).collect();
        prop_assert_eq!(ids, expected);
    }

    #[test]
    fn filtered_subset_of_unfiltered(
        n in 1usize..30,
        dim in 1usize..8,
        k in 1usize..30,
        mask in proptest::collection::vec(any::<bool>(), 1..30),
    ) {
        let metric = DistanceMetric::Euclidean;
        let mut idx = FlatIndex::new(dim, metric, FlatConfig).unwrap();
        for i in 0..n {
            let flag = *mask.get(i % mask.len()).unwrap_or(&false);
            let meta: Metadata = [(
                "flag".to_string(),
                Value::Bool(flag),
            )]
            .into_iter()
            .collect();
            let row = deterministic_row(i as u64 + 3, dim);
            idx.insert(VectorId::from(i as u64), arc(&row), Some(meta)).unwrap();
        }
        let query = deterministic_row(123, dim);

        let unfiltered = idx
            .search(&query, &SearchParams::new(k.max(n), metric))
            .unwrap();
        let filtered_params = SearchParams {
            filter: Some(Filter::eq("flag", Value::Bool(true))),
            ..SearchParams::new(k.max(n), metric)
        };
        let filtered = idx.search(&query, &filtered_params).unwrap();

        let unfiltered_ids: std::collections::HashSet<VectorId> =
            unfiltered.iter().map(|h| h.id.clone()).collect();
        for hit in &filtered {
            prop_assert!(
                unfiltered_ids.contains(&hit.id),
                "filtered hit not in unfiltered set"
            );
        }
    }
}
