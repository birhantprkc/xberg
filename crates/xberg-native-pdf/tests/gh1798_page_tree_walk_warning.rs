//! GH#1798: `get_page_from_tree_inner` used the same `Err(InvalidPdf(_))` variant
//! for "not found under this branch, keep walking" as it did for a genuine
//! page-tree fault. af54dbc621 (the GH#1755 deep-page-tree fix) folded a
//! previously silent `Err(_) => continue` into a logging arm, so a perfectly
//! valid PDF logged "error walking to page in tree; skipping branch" at WARN
//! once for every branch the walk passed before reaching the target page — up
//! to n(n-1)/2 times for a flat n-page tree, capped at 2,016 by the 64-page
//! `LAZY_THRESHOLD`.
//!
//! Captures tracing events with a thread-local subscriber (installed only for
//! the duration of the closure passed to `with_default`), never a process-global
//! sink, so this test is safe under concurrent execution with anything else. ~keep

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::Level;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt as _;
use xberg_native_pdf::PdfDocument;

const PAGE_TREE_WALK_WARNING: &str = "error walking to page in tree; skipping branch";

#[derive(Clone, Debug)]
struct CapturedEvent {
    level: Level,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct EventCapture(Arc<Mutex<Vec<CapturedEvent>>>);

struct FieldCapture<'a>(&'a mut BTreeMap<String, String>);

impl tracing::field::Visit for FieldCapture<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S> Layer<S> for EventCapture
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _context: tracing_subscriber::layer::Context<'_, S>) {
        let mut fields = BTreeMap::new();
        event.record(&mut FieldCapture(&mut fields));
        self.0.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            fields,
        });
    }
}

fn capture_events<T>(operation: impl FnOnce() -> T) -> (T, Vec<CapturedEvent>) {
    let capture = EventCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let result = tracing::subscriber::with_default(subscriber, operation);
    let events = capture.0.lock().unwrap().clone();
    (result, events)
}

fn count_matching(events: &[CapturedEvent], level: Level, needle: &str) -> usize {
    events
        .iter()
        .filter(|event| event.level == level && event.fields.get("message").is_some_and(|m| m.contains(needle)))
        .count()
}

/// A flat n-page PDF: one `/Pages` node listing every `/Page` directly as a kid,
/// matching the shape of the issue's pikepdf reproducer.
fn build_flat_multi_page_pdf(page_count: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    let kids: String = (0..page_count)
        .map(|i| format!("{} 0 R", i + 3))
        .collect::<Vec<_>>()
        .join(" ");
    pdf.extend_from_slice(
        format!("2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\nendobj\n").as_bytes(),
    );

    for _ in 0..page_count {
        offsets.push(pdf.len());
        let id = offsets.len();
        pdf.extend_from_slice(
            format!("{id} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n").as_bytes(),
        );
    }

    let xref_off = pdf.len();
    let total = offsets.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes(),
    );
    pdf
}

fn assert_every_page_resolves(doc: &PdfDocument, page_count: usize) {
    for index in 0..page_count {
        let page = doc
            .get_page(index)
            .unwrap_or_else(|error| panic!("page {index}: {error:?}"));
        let dict = page.as_dict().unwrap();
        assert!(dict.contains_key("MediaBox"), "page {index} lost its /MediaBox");
    }
}

#[test]
fn should_not_warn_walking_a_valid_flat_10_page_tree() {
    // 10 pages -> 45 ordinary "not found here" steps under the pre-fix code (n(n-1)/2). ~keep
    let doc = PdfDocument::from_bytes(build_flat_multi_page_pdf(10)).unwrap();
    assert_eq!(doc.page_count().unwrap(), 10);

    let ((), events) = capture_events(|| assert_every_page_resolves(&doc, 10));

    let warning_count = count_matching(&events, Level::WARN, PAGE_TREE_WALK_WARNING);
    assert_eq!(
        warning_count, 0,
        "a valid flat page tree must not log '{PAGE_TREE_WALK_WARNING}'; got {warning_count}: {events:#?}"
    );
}

#[test]
fn should_not_warn_walking_a_valid_flat_40_page_tree() {
    // Matches the issue's reproducer shape: 40 pages -> 780 warnings pre-fix (n(n-1)/2). ~keep
    let doc = PdfDocument::from_bytes(build_flat_multi_page_pdf(40)).unwrap();
    assert_eq!(doc.page_count().unwrap(), 40);

    let ((), events) = capture_events(|| assert_every_page_resolves(&doc, 40));

    let warning_count = count_matching(&events, Level::WARN, PAGE_TREE_WALK_WARNING);
    assert_eq!(
        warning_count,
        0,
        "a valid flat 40-page tree must not log '{PAGE_TREE_WALK_WARNING}'; got {warning_count} of {} WARN events",
        events.iter().filter(|event| event.level == Level::WARN).count()
    );
}

/// Negative control: a genuinely malformed branch (a `/Kids` entry pointing at a
/// non-dictionary object) must still warn. Without this, a fix that silenced the
/// arm entirely would pass the positive tests above for the wrong reason.
#[test]
fn should_still_warn_once_on_a_genuinely_malformed_branch() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    // Kid 5 is a bare integer, not a dictionary - a real structural fault, not an
    // ordinary "wrong page" mismatch.
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 5 0 R 4 0 R] /Count 3 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n");
    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n");
    let off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n42\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in [off1, off2, off3, off4, off5] {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    // Page index 1 in reading order is object 4, reached only past the malformed
    // kid 5 - a real fault the walk must still recover from.
    let (page, events) = capture_events(|| doc.get_page(1));
    assert!(
        page.is_ok(),
        "the walk must still recover past the malformed kid: {page:?}"
    );

    let warning_count = count_matching(&events, Level::WARN, PAGE_TREE_WALK_WARNING);
    assert_eq!(
        warning_count, 1,
        "a genuinely malformed branch must still warn exactly once; got {warning_count}: {events:#?}"
    );
}
