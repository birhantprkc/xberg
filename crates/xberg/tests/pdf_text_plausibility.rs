//! Language/dictionary-plausibility detection (issue #1696) against the real corpus.
//!
//! Every config leaves `ocr` unset, so these check the `implausible_text_pages` metadata
//! field on its own, without an OCR backend registered to act on it. Fixtures are read
//! directly from `test_documents/`, which is bucket-fetched and not committed (see the
//! `test-corpus` skill); a test whose fixture is not present prints a greppable `SKIP:` line
//! and returns rather than failing the run.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(all(feature = "pdf", any(feature = "ocr", feature = "ocr-pipeline")))]

use std::path::{Path, PathBuf};

use xberg::core::config::{ExtractInput, ExtractionConfig};
use xberg::types::FormatMetadata;

fn corpus(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_documents")
        .join(relative)
}

/// Extract a corpus PDF and return `(page_count, implausible_text_pages)`, or `None` when the
/// fixture is not materialized locally.
async fn plausibility_metadata(relative: &str, config: &ExtractionConfig) -> Option<(usize, Option<Vec<u32>>)> {
    let path = corpus(relative);
    if !path.exists() {
        println!("SKIP: fixture {relative} not available");
        return None;
    }

    let input = ExtractInput::from_uri(path.to_string_lossy().into_owned());
    let result = xberg::extract(input, config)
        .await
        .unwrap_or_else(|error| panic!("extraction failed for {relative}: {error}"));

    assert!(result.errors.is_empty(), "{relative}: {:?}", result.errors);
    let document = result
        .results
        .first()
        .unwrap_or_else(|| panic!("{relative}: no result"));

    let page_count = document.counts.pages;
    let implausible = match &document.metadata.format {
        Some(FormatMetadata::Pdf(pdf)) => pdf.implausible_text_pages.clone(),
        other => panic!("expected PDF metadata for {relative}, got {other:?}"),
    };
    Some((page_count, implausible))
}

/// Every page of a `wrong_mapping` fixture carries a `/ToUnicode` CMap rewritten to a ROT-3
/// permutation (see `test_documents/pdf/wrong_mapping/PROVENANCE.md`): the decoded text is
/// wrong on every page, so every document must have at least one page flagged.
///
/// NOT a per-page assertion (measured, not assumed): `wrong_mapping_276728418.pdf` page 2 is a
/// short tail section (trademark/security boilerplate) whose prose content, after excluding
/// its bulleted lines, is only ~449 characters -- below the 3-chunk (600-character) floor the
/// detector requires before rendering any verdict at all (`PLAUSIBILITY_MIN_PROSE_CHUNKS`,
/// `extractors::pdf::ocr::plausibility`). That page legitimately abstains
/// (`PlausibilityVerdict::NotEvaluated`) rather than guess on too little evidence; this is the
/// same "decline to judge" posture the character-shape gate already uses elsewhere in this
/// crate, not a routing miss. Page 1 of the same document (798 prose characters) IS flagged.
/// See the design doc's own step 8 acceptance note for the corresponding calibration finding. ~keep
#[tokio::test]
async fn wrong_mapping_fixtures_flag_at_least_one_page() {
    let config = ExtractionConfig::default();
    let mut evaluated = 0;

    for fixture in [
        "pdf/wrong_mapping/wrong_mapping_177210299.pdf",
        "pdf/wrong_mapping/wrong_mapping_276728418.pdf",
        "pdf/wrong_mapping/wrong_mapping_290573779.pdf",
    ] {
        let Some((page_count, implausible)) = plausibility_metadata(fixture, &config).await else {
            continue;
        };
        evaluated += 1;

        let implausible = implausible.unwrap_or_else(|| panic!("{fixture}: no implausible_text_pages"));
        println!(
            "{fixture}: {}/{page_count} page(s) flagged implausible: {implausible:?}",
            implausible.len()
        );
        assert!(
            !implausible.is_empty(),
            "{fixture}: expected at least one of {page_count} page(s) to be flagged implausible"
        );
    }

    if evaluated == 0 {
        println!("SKIP: no wrong_mapping fixtures available; nothing evaluated");
    }
}

/// A genuine, correctly-mapped born-digital prose document: no page should read as
/// implausible.
#[tokio::test]
async fn born_digital_prose_fixture_has_no_implausible_pages() {
    let Some((_, implausible)) = plausibility_metadata("pdf/187095617.pdf", &ExtractionConfig::default()).await else {
        println!("SKIP: born-digital prose fixture not available");
        return;
    };

    assert_eq!(
        implausible.as_deref(),
        Some(&[][..]),
        "a correctly-mapped born-digital prose document must not be flagged implausible"
    );
}

/// A FinTabNet table page: mostly numeric/tabular content, correctly mapped. The detector
/// must abstain on non-prose content rather than guess, so no page should be flagged.
#[tokio::test]
async fn fintabnet_table_fixture_has_no_implausible_pages() {
    let Some((_, implausible)) =
        plausibility_metadata("pdf/ft_BKNG_2012_page_109_t0.pdf", &ExtractionConfig::default()).await
    else {
        println!("SKIP: FinTabNet table fixture not available");
        return;
    };

    assert_eq!(
        implausible.as_deref(),
        Some(&[][..]),
        "a correctly-mapped table page must not be flagged implausible"
    );
}
