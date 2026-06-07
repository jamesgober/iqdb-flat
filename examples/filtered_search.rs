//! Metadata pre-filtering: restrict the scan to rows whose metadata matches.
//!
//! The filter is evaluated **before** distance computation, so a selective
//! filter skips distance work in proportion to how much it rejects. Filter
//! semantics are closed-world — a row with no metadata never matches a positive
//! predicate.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example filtered_search
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Filter, Metadata, Result, SearchParams, Value, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

fn meta(lang: &str, year: i64) -> Metadata {
    [
        ("lang".to_string(), Value::String(lang.into())),
        ("year".to_string(), Value::Int(year)),
    ]
    .into_iter()
    .collect()
}

fn main() -> Result<()> {
    let mut idx = FlatIndex::new(2, DistanceMetric::Euclidean, FlatConfig)?;
    idx.insert(
        VectorId::from(1u64),
        arc(&[0.1, 0.0]),
        Some(meta("rust", 2026)),
    )?;
    idx.insert(
        VectorId::from(2u64),
        arc(&[0.2, 0.0]),
        Some(meta("rust", 2019)),
    )?;
    idx.insert(
        VectorId::from(3u64),
        arc(&[0.0, 0.1]),
        Some(meta("go", 2026)),
    )?;
    idx.insert(VectorId::from(4u64), arc(&[0.0, 0.2]), None)?; // no metadata

    // Unfiltered: pure nearest-neighbour over everything.
    let all = idx.search(
        &[0.0, 0.0],
        &SearchParams::new(4, DistanceMetric::Euclidean),
    )?;
    println!("unfiltered: {} hits", all.len());
    assert_eq!(all.len(), 4);

    // Filtered: lang == "rust" AND year > 2020. Only id 1 qualifies.
    let params = SearchParams {
        filter: Some(Filter::and(vec![
            Filter::eq("lang", Value::String("rust".into())),
            Filter::gt("year", Value::Int(2020)),
        ])),
        ..SearchParams::new(4, DistanceMetric::Euclidean)
    };
    let filtered = idx.search(&[0.0, 0.0], &params)?;

    println!("filtered (rust & year>2020):");
    for hit in &filtered {
        println!("  id={} distance={:.4}", hit.id, hit.distance);
    }
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, VectorId::U64(1));

    Ok(())
}
