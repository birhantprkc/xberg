//! GH#1750: `init_thread_pools` must not release a second concurrent caller
//! before the global Rayon pool it configures actually exists.
//!
//! This lives in its own test binary (its own OS process) deliberately: the
//! bug only reproduces when this test's own callers are the very first ones
//! in the process to reach `init_thread_pools` and build the process-wide
//! Rayon pool. `POOL_INIT` is a `Once` — it fires exactly once per process —
//! so any other test that reached extraction earlier in the same process
//! would already have built the pool, and this test would pass whether the
//! fence is correct or not. Keep this file to a single test for the same
//! reason. ~keep

#![cfg(feature = "pdf")]

use std::sync::{Arc, Barrier};

use xberg::core::config::{ConcurrencyConfig, ExtractionConfig};

mod helpers;
use helpers::extract_bytes_document_blocking;

/// Extractions started together, all racing `init_thread_pools`'s `call_once`
/// fence. Large enough that, on the pre-fix code, at least one caller released
/// before the winner's `build_global()` completed reached its own `par_iter` —
/// native PDF scan detection (`pdf::scan_detect::map_pages`) runs one on every
/// native PDF extraction, scanned or not — inside that window, installing
/// Rayon's default pool ahead of the configured one.
const CONCURRENT_EXTRACTIONS: usize = 32;

/// A thread budget chosen to differ from both the host's real core count and
/// Rayon's own default (`std::thread::available_parallelism`), so a pool built
/// with either of those instead of the configured budget is distinguishable
/// from the correct one, on any host this test runs on.
fn distinct_thread_budget() -> usize {
    if num_cpus::get() > 1 { 1 } else { 2 }
}

/// A minimal, valid, single-page PDF with no content stream: structurally
/// enough for `xberg_native_pdf` to open and report one page, which is all
/// this test needs to reach scan detection's `par_iter`.
fn minimal_one_page_pdf() -> Vec<u8> {
    let mut buf = Vec::<u8>::new();

    buf.extend_from_slice(b"%PDF-1.4\n");

    let obj1_offset = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

    let obj2_offset = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<</Type /Pages /Kids [3 0 R] /Count 1>>\nendobj\n");

    let obj3_offset = buf.len();
    buf.extend_from_slice(b"3 0 obj\n<</Type /Page /MediaBox [0 0 612 792] /Parent 2 0 R>>\nendobj\n");

    let xref_offset = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n");
    buf.extend_from_slice(b"0000000000 65535 f \n");
    buf.extend_from_slice(format!("{obj1_offset:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{obj2_offset:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{obj3_offset:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<</Size 4 /Root 1 0 R>>\n");
    buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    buf
}

#[test]
fn two_concurrent_extractions_apply_the_configured_thread_budget() {
    let budget = distinct_thread_budget();
    let config = ExtractionConfig {
        concurrency: Some(ConcurrencyConfig {
            max_threads: Some(budget),
            max_concurrent_ocr: None,
        }),
        ..Default::default()
    };
    let pdf_bytes = minimal_one_page_pdf();
    let barrier = Arc::new(Barrier::new(CONCURRENT_EXTRACTIONS));

    let handles: Vec<_> = (0..CONCURRENT_EXTRACTIONS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let config = config.clone();
            let pdf_bytes = pdf_bytes.clone();
            std::thread::spawn(move || {
                barrier.wait();
                extract_bytes_document_blocking(&pdf_bytes, "application/pdf", &config)
            })
        })
        .collect();

    for handle in handles {
        let result = handle.join().expect("extraction thread must not panic");
        assert!(result.is_ok(), "extraction should succeed: {:?}", result.err());
    }

    assert_eq!(
        rayon::current_num_threads(),
        budget,
        "the global rayon pool must be built with the configured thread budget ({budget}), not \
         rayon's default; a concurrent caller was released before build_global() completed and \
         installed the default pool first"
    );
}
