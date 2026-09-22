//! Shared test-only fixtures for `derive::tests` and `derive::tests_extraction_result`.

use super::*;

/// Helper: create a minimal internal document.
pub(super) fn make_doc(source_format: &'static str) -> InternalDocument {
    InternalDocument::new(source_format)
}
