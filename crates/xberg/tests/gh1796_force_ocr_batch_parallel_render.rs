//! Regression test for https://github.com/xberg-io/xberg/issues/1796
//!
//! `force_ocr: true` used to render every OCR batch's pages sequentially on one thread
//! (`render_full_pdf_ocr_batch`), unlike the sibling `force_ocr_pages` route
//! (`render_selected_pages_from_document`, fixed for #1666), which already renders pages in
//! parallel. The thread-dispatch mechanism itself is proven by
//! `parallel_full_ocr_batch_render_dispatches_across_more_than_one_thread` in
//! `crates/xberg/src/extractors/pdf/ocr/tests.rs` (it needs access to private render-thread
//! instrumentation that only exists inside the crate). This file instead proves the fix is
//! output-transparent through the public API: each page's rendered raster must still land on
//! its own page number, in order, and `force_ocr` must agree byte-for-byte with
//! `force_ocr_pages` on the same input -- the exact invariant a botched parallel `collect()`
//! (a swap, an off-by-one, a race) would break.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test binaries print by design; org logging policy exempts tests
#![cfg(all(feature = "pdf", feature = "ocr"))]

mod helpers;
use helpers::extract_bytes_document_blocking;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use xberg::core::config::{ConcurrencyConfig, ExtractionConfig, OcrConfig, PageConfig};
use xberg::plugins::{OcrBackend, OcrBackendType, Plugin, register_ocr_backend, unregister_ocr_backend};
use xberg::types::{ExtractedDocument, PageContent};

/// Small enough to render fast, large enough that a single OCR batch spans every page
/// (`ConcurrencyConfig::max_threads` below is set to this count).
const PAGE_COUNT: usize = 6;
/// Fixed page height (pt); each page's WIDTH varies instead, so the rendered raster's own
/// aspect ratio uniquely identifies which page it came from.
const PAGE_HEIGHT_PT: u32 = 200;
/// Distinct, monotonically increasing widths (pt), one per page. A page whose raster lands
/// under the wrong page number -- a swap or off-by-one from a broken parallel collect -- shows
/// up as a ratio out of this order.
const PAGE_WIDTHS_PT: [u32; PAGE_COUNT] = [100, 140, 180, 220, 260, 300];
/// Largest relative error tolerated between a page's rendered aspect ratio and its own
/// `MediaBox` ratio: DPI-driven pixel rounding on a small page can move the ratio by a
/// fraction of a percent even when nothing is wrong.
const RATIO_TOLERANCE: f64 = 0.05;

/// A hand-written minimal PDF (no external PDF-writer crate needed, matching the shape
/// `crate::extractors::pdf::ocr::tests::build_minimal_multi_page_pdf_with_media_box` uses
/// internally): a Catalog/Pages/Page object graph with its own xref table, one content-free
/// page per `widths_pt` entry. No image XObject is embedded -- a page with no XObjects never
/// enters the blank-page image-XObject OCR fallback (#1355), so the mock backend always sees
/// the actual rendered full-page raster rather than a recovered embedded image. Page identity
/// for this test comes entirely from each page's distinct `MediaBox` size. ~keep
fn build_multi_page_pdf(widths_pt: &[u32], height_pt: u32) -> Vec<u8> {
    let page_count = widths_pt.len();
    let mut buf = Vec::<u8>::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::new();

    offsets.push(buf.len());
    buf.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

    offsets.push(buf.len());
    let kids: String = (0..page_count).map(|i| format!("{} 0 R ", 3 + i)).collect();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<</Type /Pages /Kids [{}] /Count {}>>\nendobj\n",
            kids.trim_end(),
            page_count
        )
        .as_bytes(),
    );

    for (i, width_pt) in widths_pt.iter().enumerate() {
        offsets.push(buf.len());
        let object_number = 3 + i;
        buf.extend_from_slice(
            format!(
                "{object_number} 0 obj\n<</Type /Page /MediaBox [0 0 {width_pt} {height_pt}] /Parent 2 0 R>>\nendobj\n"
            )
            .as_bytes(),
        );
    }

    // Object 0 is the free-list head every xref table starts with; objects 1..=2+page_count are
    // the real objects written above, so `Size` (and the xref subsection's count) is one more
    // than that highest object number.
    let object_count_including_free_head = 2 + page_count + 1;
    let xref_offset = buf.len();
    buf.extend_from_slice(b"xref\n");
    buf.extend_from_slice(format!("0 {object_count_including_free_head}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(format!("trailer\n<</Size {object_count_including_free_head} /Root 1 0 R>>\n").as_bytes());
    buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    buf
}

/// An OCR backend that reports the decoded raster's own pixel dimensions instead of real OCR
/// text, so a page's content in the extraction result directly betrays which page's raster it
/// actually came from -- the signal a page-order/off-by-one regression in a parallel render
/// collect would corrupt.
struct DimensionEchoOcrBackend {
    name: &'static str,
}

#[async_trait]
impl OcrBackend for DimensionEchoOcrBackend {
    fn backend_type(&self) -> OcrBackendType {
        OcrBackendType::Custom
    }

    fn supports_language(&self, _language: &str) -> bool {
        true
    }

    async fn process_image(&self, image_bytes: &[u8], _config: &OcrConfig) -> xberg::Result<ExtractedDocument> {
        let image = image::load_from_memory(image_bytes)
            .expect("the OCR pipeline must hand this backend a decodable rendered-page PNG");
        let mut document = ExtractedDocument::default();
        document.content = format!("{}x{}", image.width(), image.height());
        Ok(document)
    }
}

impl Plugin for DimensionEchoOcrBackend {
    fn name(&self) -> &str {
        self.name
    }

    fn version(&self) -> String {
        "0.0.0".to_string()
    }

    fn initialize(&self) -> xberg::Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> xberg::Result<()> {
        Ok(())
    }
}

struct BackendGuard(&'static str);

impl Drop for BackendGuard {
    fn drop(&mut self) {
        let _ = unregister_ocr_backend(self.0);
    }
}

fn register_backend(name: &'static str) -> BackendGuard {
    register_ocr_backend(std::sync::Arc::new(DimensionEchoOcrBackend { name })).unwrap();
    BackendGuard(name)
}

/// Parses the mock backend's `"WIDTHxHEIGHT"` content back into an aspect ratio.
fn parse_dimension_ratio(content: &str) -> f64 {
    let (width, height) = content
        .trim()
        .split_once('x')
        .unwrap_or_else(|| panic!("mock backend content must be \"WIDTHxHEIGHT\", got {content:?}"));
    let width: f64 = width.parse().expect("width must parse as a number");
    let height: f64 = height.parse().expect("height must parse as a number");
    width / height
}

/// Digest over page number + content only (never the whole `ExtractedDocument`, which also
/// carries timing/metadata fields that legitimately vary run to run).
fn pages_digest(pages: &[PageContent]) -> String {
    let mut hasher = Sha256::new();
    for page in pages {
        hasher.update(page.page_number.to_le_bytes());
        hasher.update(page.content.as_bytes());
    }
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn base_config(backend_name: &str) -> ExtractionConfig {
    ExtractionConfig {
        ocr: Some(OcrConfig {
            backend: backend_name.to_string(),
            ..Default::default()
        }),
        pages: Some(PageConfig {
            extract_pages: true,
            ..Default::default()
        }),
        concurrency: Some(ConcurrencyConfig {
            max_threads: Some(PAGE_COUNT),
            max_concurrent_ocr: None,
        }),
        use_cache: false,
        ..Default::default()
    }
}

/// Cross-route parity: `force_ocr` (the route this issue fixes, `render_full_pdf_ocr_batch`)
/// and `force_ocr_pages` (the sibling route, already parallel since #1666,
/// `render_selected_pages_from_document`) must OCR the same pages, in the same order, with the
/// same content, on the same input. A page-order bug introduced while parallelizing
/// `force_ocr`'s render batch would show up here as a mismatch against the (unchanged,
/// already-correct) `force_ocr_pages` route.
#[test]
#[serial_test::serial]
fn force_ocr_and_force_ocr_pages_agree_on_page_content_and_order() {
    const BACKEND_NAME: &str = "gh1796-dimension-echo-parity";
    let _backend = register_backend(BACKEND_NAME);

    let pdf_bytes = build_multi_page_pdf(&PAGE_WIDTHS_PT, PAGE_HEIGHT_PT);

    let force_ocr_config = ExtractionConfig {
        force_ocr: true,
        ..base_config(BACKEND_NAME)
    };
    let force_ocr_pages_config = ExtractionConfig {
        force_ocr_pages: Some((1..=PAGE_COUNT as u32).collect()),
        ..base_config(BACKEND_NAME)
    };

    let force_ocr_result = extract_bytes_document_blocking(&pdf_bytes, "application/pdf", &force_ocr_config)
        .expect("force_ocr extraction must succeed");
    let force_ocr_pages_result =
        extract_bytes_document_blocking(&pdf_bytes, "application/pdf", &force_ocr_pages_config)
            .expect("force_ocr_pages extraction must succeed");

    let force_ocr_pages_out = force_ocr_result
        .pages
        .as_ref()
        .expect("force_ocr must produce page contents");
    let force_ocr_pages_pages_out = force_ocr_pages_result
        .pages
        .as_ref()
        .expect("force_ocr_pages must produce page contents");

    assert_eq!(
        force_ocr_pages_out.len(),
        PAGE_COUNT,
        "force_ocr must OCR every page: {force_ocr_pages_out:?}"
    );
    assert_eq!(
        force_ocr_pages_pages_out.len(),
        PAGE_COUNT,
        "force_ocr_pages must OCR every page: {force_ocr_pages_pages_out:?}"
    );

    for (i, width_pt) in PAGE_WIDTHS_PT.iter().enumerate() {
        let expected_ratio = f64::from(*width_pt) / f64::from(PAGE_HEIGHT_PT);
        for (route, page) in [
            ("force_ocr", &force_ocr_pages_out[i]),
            ("force_ocr_pages", &force_ocr_pages_pages_out[i]),
        ] {
            assert_eq!(
                page.page_number,
                (i + 1) as u32,
                "{route}: page {} landed at the wrong page_number",
                i + 1
            );
            let ratio = parse_dimension_ratio(&page.content);
            assert!(
                (ratio - expected_ratio).abs() / expected_ratio < RATIO_TOLERANCE,
                "{route}: page {} rendered raster aspect ratio {ratio:.4} does not match its own \
                 MediaBox ratio {expected_ratio:.4} (content: {:?}) -- a page collected out of \
                 order (or from the wrong render call) would land here",
                i + 1,
                page.content
            );
        }
    }

    let force_ocr_digest = pages_digest(force_ocr_pages_out);
    let force_ocr_pages_digest = pages_digest(force_ocr_pages_pages_out);
    assert_eq!(
        force_ocr_digest, force_ocr_pages_digest,
        "force_ocr and force_ocr_pages must produce byte-identical page content on the same \
         input;\nforce_ocr pages: {force_ocr_pages_out:?}\nforce_ocr_pages pages: {force_ocr_pages_pages_out:?}"
    );
}

/// `force_ocr`'s parallel render batch must be deterministic: two runs of the same extraction
/// must produce the same page content in the same order. A data race in a broken parallel
/// `collect()` (as opposed to a merely-wrong-but-stable bug) would show up as run-to-run
/// nondeterminism rather than a fixed disagreement with `force_ocr_pages`.
#[test]
#[serial_test::serial]
fn force_ocr_batch_render_output_is_stable_across_repeated_runs() {
    const BACKEND_NAME: &str = "gh1796-dimension-echo-stability";
    let _backend = register_backend(BACKEND_NAME);

    let pdf_bytes = build_multi_page_pdf(&PAGE_WIDTHS_PT, PAGE_HEIGHT_PT);
    let config = ExtractionConfig {
        force_ocr: true,
        ..base_config(BACKEND_NAME)
    };

    let first = extract_bytes_document_blocking(&pdf_bytes, "application/pdf", &config)
        .expect("first force_ocr extraction must succeed");
    let second = extract_bytes_document_blocking(&pdf_bytes, "application/pdf", &config)
        .expect("second force_ocr extraction must succeed");

    let first_pages = first.pages.as_ref().expect("first run must produce page contents");
    let second_pages = second.pages.as_ref().expect("second run must produce page contents");

    assert_eq!(
        first_pages.len(),
        PAGE_COUNT,
        "first run must OCR every page: {first_pages:?}"
    );
    assert_eq!(
        second_pages.len(),
        PAGE_COUNT,
        "second run must OCR every page: {second_pages:?}"
    );
    assert_eq!(
        pages_digest(first_pages),
        pages_digest(second_pages),
        "parallel rendering must not introduce run-to-run nondeterminism in page content/order; \
         run 1: {first_pages:?}\nrun 2: {second_pages:?}"
    );
}
