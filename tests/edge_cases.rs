//! Edge-case coverage for `FlatIndex`.

#![allow(clippy::unwrap_used)]

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, IqdbError, Metadata, SearchParams, Value, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn new_empty(dim: usize, metric: DistanceMetric) -> FlatIndex {
    FlatIndex::new(dim, metric, FlatConfig).unwrap()
}

#[test]
fn new_zero_dim_is_invalid_config() {
    let err = FlatIndex::new(0, DistanceMetric::Euclidean, FlatConfig).unwrap_err();
    assert!(
        matches!(err, IqdbError::InvalidConfig { .. }),
        "expected InvalidConfig, got {err:?}",
    );
}

#[test]
fn search_on_empty_index_returns_empty() {
    let idx = new_empty(3, DistanceMetric::Euclidean);
    let hits = idx
        .search(
            &[0.0, 0.0, 0.0],
            &SearchParams::new(5, DistanceMetric::Euclidean),
        )
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_with_k_zero_returns_empty() {
    let mut idx = new_empty(2, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[0.0, 0.0]), None)
        .unwrap();
    let hits = idx
        .search(
            &[0.0, 0.0],
            &SearchParams::new(0, DistanceMetric::Euclidean),
        )
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_with_k_greater_than_n_returns_all() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[1.0]), None).unwrap();
    idx.insert(VectorId::from(2u64), arc(&[2.0]), None).unwrap();
    let hits = idx
        .search(&[0.0], &SearchParams::new(100, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, VectorId::U64(1));
    assert_eq!(hits[1].id, VectorId::U64(2));
}

#[test]
fn search_with_k_equal_n_returns_all() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[1.0]), None).unwrap();
    idx.insert(VectorId::from(2u64), arc(&[2.0]), None).unwrap();
    let hits = idx
        .search(&[0.0], &SearchParams::new(2, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits.len(), 2);
}

#[test]
fn insert_dimension_mismatch_returns_typed_error() {
    let mut idx = new_empty(3, DistanceMetric::Euclidean);
    let err = idx
        .insert(VectorId::from(1u64), arc(&[0.0, 0.0]), None)
        .unwrap_err();
    assert_eq!(
        err,
        IqdbError::DimensionMismatch {
            expected: 3,
            found: 2,
        }
    );
}

#[test]
fn search_dimension_mismatch_returns_typed_error() {
    let idx = new_empty(3, DistanceMetric::Euclidean);
    let err = idx
        .search(
            &[0.0, 0.0],
            &SearchParams::new(1, DistanceMetric::Euclidean),
        )
        .unwrap_err();
    assert_eq!(
        err,
        IqdbError::DimensionMismatch {
            expected: 3,
            found: 2,
        }
    );
}

#[test]
fn search_metric_mismatch_returns_invalid_metric() {
    let idx = new_empty(2, DistanceMetric::Euclidean);
    let err = idx
        .search(&[0.0, 0.0], &SearchParams::new(1, DistanceMetric::Cosine))
        .unwrap_err();
    assert_eq!(err, IqdbError::InvalidMetric);
}

#[test]
fn insert_duplicate_id_returns_duplicate() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[0.0]), None).unwrap();
    let err = idx
        .insert(VectorId::from(1u64), arc(&[1.0]), None)
        .unwrap_err();
    assert_eq!(err, IqdbError::Duplicate);
}

#[test]
fn delete_missing_returns_not_found() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    let err = idx.delete(&VectorId::from(99u64)).unwrap_err();
    assert_eq!(err, IqdbError::NotFound);
}

#[test]
fn delete_then_search_excludes_id_and_reinsert_succeeds() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[1.0]), None).unwrap();
    idx.insert(VectorId::from(2u64), arc(&[2.0]), None).unwrap();
    idx.delete(&VectorId::from(1u64)).unwrap();
    let hits = idx
        .search(&[0.0], &SearchParams::new(10, DistanceMetric::Euclidean))
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, VectorId::U64(2));

    idx.insert(VectorId::from(1u64), arc(&[5.0]), None).unwrap();
    assert_eq!(idx.len(), 2);
}

#[test]
fn filtered_search_excludes_non_matching() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    let meta_a: Metadata = [("kind".to_string(), Value::String("a".into()))]
        .into_iter()
        .collect();
    let meta_b: Metadata = [("kind".to_string(), Value::String("b".into()))]
        .into_iter()
        .collect();
    idx.insert(VectorId::from(1u64), arc(&[0.0]), Some(meta_a))
        .unwrap();
    idx.insert(VectorId::from(2u64), arc(&[1.0]), Some(meta_b))
        .unwrap();
    idx.insert(VectorId::from(3u64), arc(&[2.0]), None).unwrap();

    let params = SearchParams {
        filter: Some(Filter::eq("kind", Value::String("a".into()))),
        ..SearchParams::new(10, DistanceMetric::Euclidean)
    };
    let hits = idx.search(&[0.0], &params).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, VectorId::U64(1));
}

#[test]
fn filtered_search_with_no_matches_returns_empty() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[0.0]), None).unwrap();
    let params = SearchParams {
        filter: Some(Filter::eq("kind", Value::String("missing".into()))),
        ..SearchParams::new(10, DistanceMetric::Euclidean)
    };
    let hits = idx.search(&[0.0], &params).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn insert_batch_fail_fast_persists_prior_inserts() {
    let mut idx = new_empty(2, DistanceMetric::Euclidean);
    let items = vec![
        (VectorId::from(1u64), arc(&[0.0, 0.0]), None),
        (VectorId::from(2u64), arc(&[1.0]), None),
        (VectorId::from(3u64), arc(&[2.0, 0.0]), None),
    ];
    let err = idx.insert_batch(items).unwrap_err();
    assert_eq!(
        err,
        IqdbError::DimensionMismatch {
            expected: 2,
            found: 1,
        }
    );
    assert_eq!(idx.len(), 1);
}

#[test]
fn flush_is_ok_for_flat() {
    let mut idx = new_empty(1, DistanceMetric::Euclidean);
    idx.flush().unwrap();
}

#[test]
fn stats_reports_flat_index_type_and_counts() {
    let mut idx = new_empty(3, DistanceMetric::Euclidean);
    idx.insert(VectorId::from(1u64), arc(&[0.0, 0.0, 0.0]), None)
        .unwrap();
    let stats = idx.stats();
    assert_eq!(stats.n_vectors, 1);
    assert_eq!(stats.index_type, "flat");
    assert_eq!(stats.disk_bytes, None);
    assert!(stats.memory_bytes > 0);
}

#[test]
fn flat_index_is_object_safe_through_dyn_index_core() {
    let mut idx: Box<dyn IndexCore> =
        Box::new(FlatIndex::new(2, DistanceMetric::Cosine, FlatConfig).unwrap());
    assert_eq!(idx.dim(), 2);
    assert_eq!(idx.metric(), DistanceMetric::Cosine);
    assert!(idx.is_empty());
    idx.insert(VectorId::from(1u64), arc(&[1.0, 0.0]), None)
        .unwrap();
    assert_eq!(idx.len(), 1);
    idx.flush().unwrap();
}
