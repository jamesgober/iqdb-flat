//! C3 — proves the seq-keyed tiebreaker survives `swap_remove`-based
//! delete sequences.
//!
//! Before C3, `FlatIndex::delete` used `Vec::remove` (shift-left,
//! preserves Vec position == insertion order); the top-`k` tiebreaker
//! could safely key off position. After C3, delete uses `swap_remove`
//! (O(1), reorders the remaining entries), and the tiebreaker keys off a
//! parallel `seqs: Vec<u64>` of monotonic insertion stamps instead. The
//! contract that callers rely on — "earlier-inserted wins on a tie,
//! across delete-then-reinsert sequences" — must be preserved exactly.
//!
//! Each test below is a scenario in which `Vec::remove` and `swap_remove`
//! would observably disagree if the tiebreaker were still position-based.
//! They are the safety net for any future change to FlatIndex storage
//! layout.

#![allow(clippy::unwrap_used)]

use iqdb_flat::{FlatConfig, FlatIndex};
use iqdb_index::{Index, IndexCore};
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

use std::sync::Arc;

fn arc(v: &[f32]) -> Arc<[f32]> {
    Arc::from(v)
}

const TIED_VECTOR: [f32; 1] = [1.0];
const QUERY: [f32; 1] = [0.0];

fn build(ids: &[u64]) -> FlatIndex {
    let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
    for id in ids {
        idx.insert(VectorId::from(*id), arc(&TIED_VECTOR), None)
            .unwrap();
    }
    idx
}

fn ids_of(idx: &FlatIndex, k: usize) -> Vec<u64> {
    idx.search(&QUERY, &SearchParams::new(k, DistanceMetric::Euclidean))
        .unwrap()
        .into_iter()
        .map(|h| match h.id {
            VectorId::U64(v) => v,
            VectorId::Bytes(_) => panic!("test only inserts U64 ids"),
        })
        .collect()
}

#[test]
fn pure_insert_order_wins_ties() {
    // No deletes: outcome must equal insertion order regardless of which
    // delete strategy the index uses internally.
    let idx = build(&[10, 20, 30, 40, 50]);
    assert_eq!(ids_of(&idx, 5), vec![10, 20, 30, 40, 50]);
}

#[test]
fn delete_from_middle_does_not_reorder_remaining() {
    // The canary scenario: insert in order, delete a middle id. The
    // remaining ids must still come out in insertion order. With the old
    // `Vec::remove` this was free (positions preserved). With
    // `swap_remove`, position 1 now holds the row that used to be at the
    // back — and `swap_remove` alone (no seq tiebreaker) would return the
    // wrong order here.
    let mut idx = build(&[10, 20, 30, 40, 50]);
    idx.delete(&VectorId::from(20u64)).unwrap();
    assert_eq!(ids_of(&idx, 4), vec![10, 30, 40, 50]);
}

#[test]
fn delete_first_does_not_reorder_remaining() {
    let mut idx = build(&[10, 20, 30, 40, 50]);
    idx.delete(&VectorId::from(10u64)).unwrap();
    assert_eq!(ids_of(&idx, 4), vec![20, 30, 40, 50]);
}

#[test]
fn delete_last_does_not_reorder_remaining() {
    let mut idx = build(&[10, 20, 30, 40, 50]);
    idx.delete(&VectorId::from(50u64)).unwrap();
    assert_eq!(ids_of(&idx, 4), vec![10, 20, 30, 40]);
}

#[test]
fn reinserted_id_sorts_last_on_ties() {
    // The historically-load-bearing case: after deleting an earlier id and
    // re-inserting it, the re-inserted id moves to the end of the tiebreak
    // order because it gets a fresh (higher) seq. This matches the
    // observable behavior the old positional ordering produced via
    // `Vec::remove` + `push`.
    let mut idx = build(&[10, 20, 30]);
    idx.delete(&VectorId::from(10u64)).unwrap();
    idx.insert(VectorId::from(10u64), arc(&TIED_VECTOR), None)
        .unwrap();
    assert_eq!(ids_of(&idx, 3), vec![20, 30, 10]);
}

#[test]
fn many_random_deletes_then_reinserts_preserve_insertion_order() {
    // Stress the swap_remove path with a fixed deterministic pattern of
    // deletes followed by re-inserts. Build with ids 0..N. Delete every
    // third id, then re-insert them; expected order = the ones never
    // deleted in their original order, followed by the re-inserted ones
    // in re-insertion order. Tests seq monotonicity across many
    // swap_remove operations.
    const N: u64 = 30;
    let ids: Vec<u64> = (0..N).collect();
    let mut idx = build(&ids);

    let deleted: Vec<u64> = ids.iter().copied().filter(|i| i % 3 == 0).collect();
    for id in &deleted {
        idx.delete(&VectorId::from(*id)).unwrap();
    }
    for id in &deleted {
        idx.insert(VectorId::from(*id), arc(&TIED_VECTOR), None)
            .unwrap();
    }

    let kept: Vec<u64> = ids.iter().copied().filter(|i| i % 3 != 0).collect();
    let expected: Vec<u64> = kept.into_iter().chain(deleted).collect();
    assert_eq!(ids_of(&idx, N as usize), expected);
}

#[test]
fn insertion_order_is_stable_across_two_independent_builds() {
    // Two completely independent FlatIndex instances built with the same
    // insertion pattern must produce identical search results — including
    // tied entries. This is the "deterministic across runs" claim the
    // tiebreaker exists to support.
    let pattern: &[(u64, &[f32])] = &[
        (7, &[2.0]),
        (3, &[2.0]),
        (11, &[2.0]),
        (1, &[2.0]),
        (9, &[2.0]),
    ];
    let build_once = || {
        let mut idx = FlatIndex::new(1, DistanceMetric::Euclidean, FlatConfig).unwrap();
        for (id, v) in pattern {
            idx.insert(VectorId::from(*id), arc(v), None).unwrap();
        }
        idx
    };
    let a = ids_of(&build_once(), 5);
    let b = ids_of(&build_once(), 5);
    assert_eq!(a, b);
    // And the order is the insertion order: 7, 3, 11, 1, 9.
    assert_eq!(a, vec![7, 3, 11, 1, 9]);
}
