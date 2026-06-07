//! Driving `FlatIndex` through `Box<dyn IndexCore>` — the engine's view.
//!
//! The engine stores a heterogeneous set of indexes as trait objects and
//! never names their concrete types after construction. `IndexCore` is
//! object-safe for exactly this reason; `Index::new` (which is generic) is
//! used only to build the concrete value, which is then boxed.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example polymorphic
//! ```

use std::sync::Arc;

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, Result, SearchParams, VectorId};

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

/// Build, populate, and query any index purely through the trait object —
/// the function has no idea it is talking to a `FlatIndex`.
fn run(index: &mut dyn IndexCore) -> Result<usize> {
    index.insert(VectorId::from(1u64), arc(&[1.0, 0.0]), None)?;
    index.insert(VectorId::from(2u64), arc(&[0.0, 1.0]), None)?;
    index.insert(VectorId::from(3u64), arc(&[0.9, 0.1]), None)?;
    index.flush()?;
    let hits = index.search(&[1.0, 0.0], &SearchParams::new(2, index.metric()))?;
    Ok(hits.len())
}

fn main() -> Result<()> {
    // Construct concretely, then erase the type behind the trait object.
    let mut index: Box<dyn IndexCore> =
        Box::new(FlatIndex::new(2, DistanceMetric::Cosine, FlatConfig)?);

    let found = run(index.as_mut())?;
    println!(
        "via dyn IndexCore: type={} metric={:?} len={} found={}",
        index.stats().index_type,
        index.metric(),
        index.len(),
        found
    );

    assert_eq!(index.stats().index_type, "flat");
    assert_eq!(index.len(), 3);
    assert_eq!(found, 2);

    Ok(())
}
