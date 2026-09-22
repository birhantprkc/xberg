//! Shared test helpers used across the subprocess adapter test suite.

use crate::types::{BatchCapability, BatchEntryPoint};

pub(super) fn test_batch_capability(per_item_timing: bool) -> BatchCapability {
    BatchCapability {
        entry_point: BatchEntryPoint::XbergCliExtractBatch,
        timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
        per_item_timing,
    }
}
