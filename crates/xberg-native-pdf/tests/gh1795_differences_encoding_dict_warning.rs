//! GH#1795: a font whose `/Encoding` is a dictionary (i.e. it carries `/Differences`
//! over a base encoding — a completely normal PDF construct) logged "dictionary used
//! where stream expected; treating as empty stream" on every load. Two sites in
//! `resolve_encoding_fields` (`crates/xberg-native-pdf/src/fonts/font_dict.rs`) called
//! `decode_stream_data` speculatively on the raw `/Encoding` object, which is a WARN
//! for any `Object::Dictionary` (`crates/xberg-native-pdf/src/object.rs`) — a warning
//! meant for a genuinely malformed PDF where a stream is stored as a bare dictionary,
//! not for an ordinary encoding dictionary that was never a stream to begin with.
//!
//! Extraction was correct throughout; only the warning was wrong. This builds two
//! one-page PDFs with the same Helvetica text — one with a `/Differences` encoding
//! dictionary, one with the plain `/WinAnsiEncoding` name — and asserts they extract
//! byte-identical text and that only the dictionary-encoding case could have regressed
//! (the name case never reaches `decode_stream_data`'s dictionary arm in the first
//! place, so it is the parity control, not a second red case).
//!
//! Captures tracing events with a thread-local subscriber (installed only for the
//! duration of the closure passed to `with_default`), never a process-global sink. ~keep

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::Level;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt as _;
use xberg_native_pdf::PdfDocument;
use xberg_native_pdf::fonts::global_cache::clear_global_font_cache;

const STREAM_EXPECTED_WARNING: &str = "dictionary used where stream expected";
const EXPECTED_TEXT: &str = "A plain line of text.";

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

enum EncodingShape {
    /// `/Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [65 /A <marker_code> /<marker_name>] >>`
    /// — a dictionary, valid PDF, and the shape that triggered GH#1795. `65 /A` is the
    /// mapping the test text actually uses; the `marker` pair is an extra, unused
    /// remapping so each caller can give its font dictionary distinct content: the
    /// global font-identity cache (`xberg_native_pdf::fonts::global_cache`) keys on
    /// resolved font content across documents, so two tests building byte-identical
    /// `/Differences` dictionaries would otherwise have the second one served from
    /// cache — skipping the very `decode_stream_data` call this test exists to
    /// observe. The marker code must not appear in the test's own content stream. ~keep
    DifferencesDict { marker_code: u8, marker_name: &'static str },
    /// `/Encoding /WinAnsiEncoding` — a bare name, the parity control.
    WinAnsiName,
}

fn build_one_page_helvetica_pdf(encoding: EncodingShape) -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 700 Td (A plain line of text.) Tj ET";
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    offsets.push(pdf.len());
    let encoding_entry = match encoding {
        EncodingShape::DifferencesDict { .. } => "6 0 R".to_string(),
        EncodingShape::WinAnsiName => "/WinAnsiEncoding".to_string(),
    };
    pdf.extend_from_slice(
        format!("5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding {encoding_entry} >>\nendobj\n")
            .as_bytes(),
    );

    if let EncodingShape::DifferencesDict {
        marker_code,
        marker_name,
    } = encoding
    {
        offsets.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "6 0 obj\n<< /Type /Encoding /BaseEncoding /WinAnsiEncoding \
                      /Differences [65 /A {marker_code} /{marker_name}] >>\nendobj\n"
            )
            .as_bytes(),
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

#[test]
fn should_not_warn_loading_a_font_with_a_differences_encoding_dictionary() {
    // Cache-key marker unique to this test - see `EncodingShape::DifferencesDict`. ~keep
    clear_global_font_cache();
    let doc = PdfDocument::from_bytes(build_one_page_helvetica_pdf(EncodingShape::DifferencesDict {
        marker_code: 90,
        marker_name: "Z",
    }))
    .unwrap();

    let (text, events) = capture_events(|| doc.extract_text(0));
    let text = text.unwrap();
    assert_eq!(
        text.trim(),
        EXPECTED_TEXT,
        "text must extract correctly despite the /Differences dictionary"
    );

    let warning_count = count_matching(&events, Level::WARN, STREAM_EXPECTED_WARNING);
    assert_eq!(
        warning_count, 0,
        "a valid /Differences encoding dictionary must not log '{STREAM_EXPECTED_WARNING}'; got {warning_count}: {events:#?}"
    );
}

/// Parity control: the same text and font with a plain `/WinAnsiEncoding` name must
/// produce byte-identical output and never warned in the first place (`as_name`
/// short-circuits before any `decode_stream_data` call). Proves the fix changes
/// only the false-positive warning, not extraction behavior.
#[test]
fn control_winansi_name_encoding_matches_differences_dict_text() {
    // Different marker than the test above, so the two tests' font dictionaries never
    // collide in the process-global font-identity cache. ~keep
    clear_global_font_cache();
    let dict_doc = PdfDocument::from_bytes(build_one_page_helvetica_pdf(EncodingShape::DifferencesDict {
        marker_code: 78,
        marker_name: "N",
    }))
    .unwrap();
    let name_doc = PdfDocument::from_bytes(build_one_page_helvetica_pdf(EncodingShape::WinAnsiName)).unwrap();

    let dict_text = dict_doc.extract_text(0).unwrap();
    let (name_text, events) = capture_events(|| name_doc.extract_text(0).unwrap());

    assert_eq!(
        dict_text, name_text,
        "the /Differences dictionary must not change extracted text"
    );
    assert_eq!(name_text.trim(), EXPECTED_TEXT);

    let warning_count = count_matching(&events, Level::WARN, STREAM_EXPECTED_WARNING);
    assert_eq!(
        warning_count, 0,
        "the name-encoding control must never warn; got {warning_count}: {events:#?}"
    );
}
