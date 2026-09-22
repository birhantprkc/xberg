//! Unit tests for the batching/partitioning free functions in `runner::helpers`.

use crate::adapter::OcrLanguagePolicy;
use crate::runner::helpers::{
    average_durations, ensure_batch_result_cardinality, fixed_batch_ranges, language_partitions,
};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn fixed_batch_ranges_are_complete_and_ordered() {
    assert_eq!(fixed_batch_ranges(8, Some(4)).unwrap(), [0..4, 4..8]);
    let legacy_batch = fixed_batch_ranges(3, None).unwrap();
    assert_eq!(legacy_batch.len(), 1);
    assert_eq!(legacy_batch[0], 0..3);
}

#[test]
fn fixed_batch_ranges_allow_smaller_final_batch() {
    // A non-multiple eligible count yields a smaller final batch rather than an error.
    assert_eq!(fixed_batch_ranges(5, Some(4)).unwrap(), [0..4, 4..5]);
    let single = fixed_batch_ranges(1, Some(4)).unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single[0], 0..1);
    // Zero documents -> no batches; a zero batch size is still rejected.
    assert!(fixed_batch_ranges(0, Some(4)).unwrap().is_empty());
    assert!(fixed_batch_ranges(0, Some(0)).is_err());
}

#[test]
fn global_language_batches_partition_without_losing_fixture_order_metadata() {
    let entries = vec![
        (0, PathBuf::from("a.png"), true, Some("eng".to_string())),
        (1, PathBuf::from("b.png"), true, Some("deu".to_string())),
        (2, PathBuf::from("c.png"), true, Some("eng".to_string())),
    ];
    let partitions = language_partitions(entries, OcrLanguagePolicy::AnyBatchGlobal);
    assert_eq!(partitions.len(), 2);
    assert_eq!(partitions[0].iter().map(|entry| entry.0).collect::<Vec<_>>(), [0, 2]);
    assert_eq!(partitions[1].iter().map(|entry| entry.0).collect::<Vec<_>>(), [1]);
}

#[test]
fn per_document_language_batches_remain_single_partition() {
    let entries = vec![
        (0, PathBuf::from("a.png"), true, Some("eng".to_string())),
        (1, PathBuf::from("b.png"), true, Some("deu".to_string())),
    ];
    assert_eq!(
        language_partitions(entries, OcrLanguagePolicy::SceptrePerDocument).len(),
        1
    );
}

#[test]
fn batch_result_cardinality_rejects_missing_and_surplus_rows() {
    assert!(ensure_batch_result_cardinality("xberg", 2, 2).is_ok());
    assert!(ensure_batch_result_cardinality("xberg", 2, 1).is_err());
    assert!(ensure_batch_result_cardinality("xberg", 2, 3).is_err());
}

#[test]
fn repeated_batch_overhead_is_averaged() {
    let overheads = [Duration::from_millis(5), Duration::from_millis(15)];

    assert_eq!(
        average_durations(overheads.into_iter()),
        Some(Duration::from_millis(10))
    );
    assert_eq!(average_durations(std::iter::empty()), None);
}
