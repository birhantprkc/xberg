//! Unit tests for [`super`].
//!
//! Split out of `document.rs` purely for file size: the parent was 28,713 lines
//! (1.2 MiB) and tripped the repository's 500 KiB file-safety limit. A child
//! module sees the parent's private items exactly as an inline `mod tests` did. ~keep

use super::*;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use tracing::Level;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt as _;

#[derive(Clone, Debug)]
struct CapturedEvent {
    level: Level,
    target: String,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct EventCapture(Arc<Mutex<Vec<CapturedEvent>>>);

impl<S> Layer<S> for EventCapture
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _context: tracing_subscriber::layer::Context<'_, S>) {
        let mut visitor = FieldCapture::default();
        event.record(&mut visitor);
        self.0.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            target: event.metadata().target().to_string(),
            fields: visitor.0,
        });
    }
}

#[derive(Default)]
struct FieldCapture(BTreeMap<String, String>);

impl tracing::field::Visit for FieldCapture {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

fn capture_events<T>(operation: impl FnOnce() -> T) -> (T, Vec<CapturedEvent>) {
    let capture = EventCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let result = tracing::subscriber::with_default(subscriber, operation);
    let events = capture.0.lock().unwrap().clone();
    (result, events)
}

#[test]
fn test_run_is_signed_number() {
    // Signed numeric exponents (unit notation) are detected for every
    // common minus/hyphen sign and left to the caller to skip. ~keep
    assert!(PdfDocument::run_is_signed_number("-1")); // U+002D hyphen-minus ~keep
    assert!(PdfDocument::run_is_signed_number("\u{2212}2")); // U+2212 minus sign ~keep
    assert!(PdfDocument::run_is_signed_number("\u{2010}3")); // U+2010 hyphen ~keep
    assert!(PdfDocument::run_is_signed_number("\u{2011}45"));
    // ~keep
    // A bare sign with no digit is not a signed number. ~keep
    assert!(!PdfDocument::run_is_signed_number("-"));
    // Unsigned digit runs (chemistry subscripts, ordinals, exponents the
    // plaintext convention DOES want as Unicode) are not affected. ~keep
    assert!(!PdfDocument::run_is_signed_number("2"));
    assert!(!PdfDocument::run_is_signed_number("th"));
    assert!(!PdfDocument::run_is_signed_number(""));
    // The sign must lead: an interior hyphen is not a signed exponent. ~keep
    assert!(!PdfDocument::run_is_signed_number("1-"));
}

/// A span injected into a tagged page via `extract_text_with_extra_spans`
/// and carrying the MCID of a middle block must be emitted at that block's
/// position in structure order — not appended after the page. This is the
/// primitive the Auto extractor uses to drop OCR'd image text into the
/// figure's reading-order slot instead of after the whole page.
#[test]
fn extra_span_with_borrowed_mcid_lands_in_structure_order() {
    // Three tagged paragraphs (MCID 0/1/2) drawn top-to-bottom. ~keep
    let content = b"BT /F1 12 Tf\n\
            /P <</MCID 0>> BDC 1 0 0 1 72 700 Tm (ALPHA) Tj EMC\n\
            /P <</MCID 1>> BDC 1 0 0 1 72 600 Tm (BRAVO) Tj EMC\n\
            /P <</MCID 2>> BDC 1 0 0 1 72 500 Tm (CHARLIE) Tj EMC\n\
            ET\n";
    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; 9];
    let obj = |buf: &mut Vec<u8>, off: &mut Vec<usize>, id: usize, body: &str| {
        off[id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    let stream = |buf: &mut Vec<u8>, off: &mut Vec<usize>, id: usize, data: &[u8]| {
        off[id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n<< /Length {} >>\nstream\n", data.len()).as_bytes());
        buf.extend_from_slice(data);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
    };
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    obj(
        &mut buf,
        &mut off,
        1,
        "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 7 0 R >>",
    );
    obj(&mut buf, &mut off, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut buf,
        &mut off,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R /StructParents 0 >>",
    );
    stream(&mut buf, &mut off, 4, content);
    obj(
        &mut buf,
        &mut off,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    // Minimal struct tree: three /P kids referencing MCID 0/1/2. ~keep
    obj(&mut buf, &mut off, 7, "<< /Type /StructTreeRoot /K [8 0 R] >>");
    obj(
        &mut buf,
        &mut off,
        8,
        "<< /Type /StructElem /S /Document /K [<< /Type /StructElem /S /P /Pg 3 0 R /K 0 >> \
             << /Type /StructElem /S /P /Pg 3 0 R /K 1 >> \
             << /Type /StructElem /S /P /Pg 3 0 R /K 2 >>] >>",
    );
    let xref = buf.len();
    buf.extend_from_slice(b"xref\n0 9\n0000000000 65535 f \n");
    for id in 1..=8 {
        if id == 6 {
            buf.extend_from_slice(b"0000000000 65535 f \n");
            continue;
        }
        buf.extend_from_slice(format!("{:010} 00000 n \n", off[id]).as_bytes());
    }
    buf.extend_from_slice(b"trailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{xref}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(buf).unwrap();
    let plain = doc.extract_text(0).unwrap();
    assert!(
        plain.find("ALPHA") < plain.find("BRAVO") && plain.find("BRAVO") < plain.find("CHARLIE"),
        "baseline structure order wrong: {plain:?}"
    );

    // Inject a span carrying MCID 1 (BRAVO's block) positioned at BRAVO's
    // y. It must land within BRAVO's group — after BRAVO, before CHARLIE —
    // NOT appended after CHARLIE. ~keep
    let extra = crate::layout::TextSpan {
        text: "INSERTED".to_string(),
        bbox: crate::geometry::Rect::new(72.0, 590.0, 50.0, 12.0),
        font_size: 12.0,
        mcid: Some(1),
        ..Default::default()
    };
    let opts = crate::converters::ConversionOptions {
        extract_tables: true,
        ..Default::default()
    };
    let out = doc.extract_text_with_extra_spans(0, vec![extra], &opts).unwrap();
    let (a, b, ins, c) = (
        out.find("ALPHA"),
        out.find("BRAVO"),
        out.find("INSERTED"),
        out.find("CHARLIE"),
    );
    assert!(ins.is_some(), "injected span dropped: {out:?}");
    assert!(
        a < b && b < ins && ins < c,
        "injected span not placed in MCID-1 slot (expected ALPHA<BRAVO<INSERTED<CHARLIE): {out:?}"
    );
}

/// An extra span dropped onto a dominant-rotation page must land in the
/// same reading-order slot on every surface: the extras merge before the
/// reading-frame map. Mapping base spans around an unmapped extra files
/// the extra text after the page in md/html but mid-page in text.
#[test]
fn extra_span_shares_the_reading_frame_on_every_surface() {
    // Three 90°-rotated lines; the mapped frame puts them at
    // y' = 612 - x: ALPHA 412, BRAVO 384, CHARLIE 356. The unrotated
    // extra at (242, 100) maps to y' = 370 — between BRAVO and CHARLIE.
    let mut content: Vec<u8> = b"BT /F1 10 Tf\n".to_vec();
    for (x, text) in [(200, "ALPHA"), (228, "BRAVO"), (256, "CHARLIE")] {
        content.extend_from_slice(format!("0 1 -1 0 {x} 150 Tm ({text}) Tj\n").as_bytes());
    }
    content.extend_from_slice(b"ET");

    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; 6];
    let obj = |buf: &mut Vec<u8>, off: &mut Vec<usize>, id: usize, body: &str| {
        off[id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    buf.extend_from_slice(b"%PDF-1.4\n");
    obj(&mut buf, &mut off, 1, "<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut buf, &mut off, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut buf,
        &mut off,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>",
    );
    off[4] = buf.len();
    buf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    buf.extend_from_slice(&content);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    obj(
        &mut buf,
        &mut off,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    let xref = buf.len();
    buf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for id in 1..=5 {
        buf.extend_from_slice(format!("{:010} 00000 n \n", off[id]).as_bytes());
    }
    buf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{xref}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(buf).unwrap();
    let extra = crate::layout::TextSpan {
        text: "INSERTED".to_string(),
        bbox: crate::geometry::Rect::new(242.0, 100.0, 50.0, 10.0),
        font_size: 10.0,
        ..Default::default()
    };
    let opts = crate::converters::ConversionOptions {
        extract_tables: true,
        ..Default::default()
    };

    let slot = |out: &str, surface: &str| {
        let (a, b, ins, c) = (
            out.find("ALPHA"),
            out.find("BRAVO"),
            out.find("INSERTED"),
            out.find("CHARLIE"),
        );
        assert!(
            a.is_some() && a < b && b < ins && ins < c,
            "{surface}: expected ALPHA<BRAVO<INSERTED<CHARLIE: {out:?}"
        );
    };
    slot(
        &doc.extract_text_with_extra_spans(0, vec![extra], &opts).unwrap(),
        "extract_text_with_extra_spans",
    );
}

#[test]
fn test_rotate_span_bbox_identity_and_180() {
    let r = crate::geometry::Rect::new(10.0, 20.0, 30.0, 5.0);
    let (w, h) = (200.0, 100.0);

    // rot == 0 is the identity (byte-identical, unrotated pages untouched). ~keep
    let id = PdfDocument::rotate_span_bbox(r, 0, w, h);
    assert!((id.x - r.x).abs() < 1e-4 && (id.y - r.y).abs() < 1e-4);
    assert!((id.width - r.width).abs() < 1e-4 && (id.height - r.height).abs() < 1e-4);

    // rot == 180 matches the legacy mirror: x' = w-(x+width), y' = h-(y+height). ~keep
    let m = PdfDocument::rotate_span_bbox(r, 180, w, h);
    assert!((m.x - (w - (r.x + r.width))).abs() < 1e-4, "180 x: {}", m.x);
    assert!((m.y - (h - (r.y + r.height))).abs() < 1e-4, "180 y: {}", m.y);
    assert!((m.width - r.width).abs() < 1e-4 && (m.height - r.height).abs() < 1e-4);
}

#[test]
fn test_rotate_span_bbox_90_270_roundtrip_and_swap() {
    let r = crate::geometry::Rect::new(10.0, 20.0, 30.0, 5.0);
    // 90° / 270° swap width and height of the AABB. ~keep
    let r90 = PdfDocument::rotate_span_bbox(r, 90, 200.0, 100.0);
    assert!((r90.width - r.height).abs() < 1e-4, "w/h swap: {}", r90.width);
    assert!((r90.height - r.width).abs() < 1e-4, "w/h swap: {}", r90.height);

    // Applying 90° four times around a square page returns to the start. ~keep
    let s = crate::geometry::Rect::new(12.0, 34.0, 6.0, 8.0);
    let p = 100.0;
    let a = PdfDocument::rotate_span_bbox(s, 90, p, p);
    let b = PdfDocument::rotate_span_bbox(a, 90, p, p);
    let c = PdfDocument::rotate_span_bbox(b, 90, p, p);
    let d = PdfDocument::rotate_span_bbox(c, 90, p, p);
    assert!((d.x - s.x).abs() < 1e-3, "roundtrip x: {} vs {}", d.x, s.x);
    assert!((d.y - s.y).abs() < 1e-3, "roundtrip y: {} vs {}", d.y, s.y);
    assert!((d.width - s.width).abs() < 1e-3 && (d.height - s.height).abs() < 1e-3);
}

#[test]
fn test_parse_valid_header_1_7() {
    let mut cursor = Cursor::new(b"%PDF-1.7\n");
    let (major, minor, offset) = parse_header(&mut cursor, false).unwrap();
    assert_eq!((major, minor, offset), (1, 7, 0));
}

#[test]
fn test_parse_valid_header_1_4() {
    let mut cursor = Cursor::new(b"%PDF-1.4");
    let (major, minor, offset) = parse_header(&mut cursor, false).unwrap();
    assert_eq!((major, minor, offset), (1, 4, 0));
}

#[test]
fn test_parse_valid_header_1_0() {
    let mut cursor = Cursor::new(b"%PDF-1.0");
    let (major, minor, offset) = parse_header(&mut cursor, false).unwrap();
    assert_eq!((major, minor, offset), (1, 0, 0));
}

#[test]
fn test_parse_valid_header_2_0() {
    let mut cursor = Cursor::new(b"%PDF-2.0");
    let (major, minor, offset) = parse_header(&mut cursor, false).unwrap();
    assert_eq!((major, minor, offset), (2, 0, 0));
}

#[test]
fn test_parse_invalid_header_wrong_magic_strict() {
    let mut cursor = Cursor::new(b"NotAPDF\n");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::InvalidHeader(_)));
}

#[test]
fn test_parse_invalid_header_unsupported_version() {
    let mut cursor = Cursor::new(b"%PDF-3.0");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::UnsupportedVersion(_)));
}

#[test]
fn test_parse_invalid_header_version_0_0() {
    let mut cursor = Cursor::new(b"%PDF-0.0");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_header_no_dot() {
    let mut cursor = Cursor::new(b"%PDF-17\n");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::InvalidHeader(_)));
}

#[test]
fn test_parse_invalid_header_too_short() {
    let mut cursor = Cursor::new(b"%PDF");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_header_non_digit_version() {
    let mut cursor = Cursor::new(b"%PDF-X.Y");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::InvalidHeader(_)));
}

#[test]
fn test_parse_header_with_bom_prefix() {
    let data = b"\xEF\xBB\xBF%PDF-1.7\n";
    let mut cursor = Cursor::new(data);
    let (major, minor, offset) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor, offset), (1, 7, 3));
}

#[test]
fn test_parse_header_with_binary_prefix() {
    let mut data = vec![0x1b, 0x96, 0x5f];
    data.extend_from_slice(b"%PDF-1.4\n");
    let mut cursor = Cursor::new(data);
    let (major, minor, offset) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor, offset), (1, 4, 3));
}

#[test]
fn test_parse_header_at_boundary() {
    // Header starting at byte 1016 (within 1024-byte window, with 8 bytes for full header)
    // ~keep
    let mut data = vec![0u8; 1016];
    data.extend_from_slice(b"%PDF-1.5");
    let mut cursor = Cursor::new(data);
    let (major, minor, offset) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor, offset), (1, 5, 1016));
}

#[test]
fn test_parse_header_not_found_lenient() {
    let data = vec![0u8; 1024];
    let mut cursor = Cursor::new(data);
    let (major, minor, offset) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor), (1, 4));
    assert_eq!(offset, 0);
}

#[test]
fn test_parse_header_strict_rejects_offset() {
    let mut data = vec![0x1b, 0x96, 0x5f];
    data.extend_from_slice(b"%PDF-1.4\n");
    let mut cursor = Cursor::new(data);
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::InvalidHeader(_)));
}

#[test]
fn test_parse_trailer_basic() {
    let data = b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n";
    let mut cursor = Cursor::new(data);
    let trailer = parse_trailer(&mut cursor).unwrap();

    let dict = trailer.as_dict().unwrap();
    assert_eq!(dict.get("Size").unwrap().as_integer(), Some(6));
    assert!(dict.get("Root").unwrap().as_reference().is_some());
}

#[test]
fn test_parse_trailer_missing_keyword() {
    let data = b"<< /Size 6 >>\nstartxref\n";
    let mut cursor = Cursor::new(data);
    let result = parse_trailer(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_parse_trailer_not_dictionary() {
    let data = b"trailer\n[ 1 2 3 ]\nstartxref\n";
    let mut cursor = Cursor::new(data);
    let result = parse_trailer(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_document_open_nonexistent_file() {
    let result = PdfDocument::open("/nonexistent/path/to/file.pdf");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::Io(_)));
}

#[test]
fn test_circular_reference_detection() {
    // This test ensures that the cycle detection mechanism works
    // We can't easily create a circular PDF in a unit test, but we can
    // verify that the error types exist and are properly defined ~keep
    use crate::object::ObjectRef;

    let obj_ref = ObjectRef::new(1, 0);
    let err = Error::CircularReference(obj_ref);
    let msg = format!("{}", err);
    assert!(msg.contains("Circular reference"));
    assert!(msg.contains("object 1 0 R"));
}

#[test]
fn test_recursion_limit_error() {
    let err = Error::RecursionLimitExceeded(100);
    let msg = format!("{}", err);
    assert!(msg.contains("Recursion depth limit exceeded"));
    assert!(msg.contains("100"));
}

/// Regression test: circular Form XObject references must not cause
/// a stack overflow / segfault. The PDF has X0→X1→X0 circular references.
#[test]
fn test_issue_163_circular_form_xobjects() {
    let pdf_bytes = build_circular_xobject_pdf();
    let dir = tempfile::tempdir().expect("create temp dir");
    let tmp_path = dir.path().join("native_pdf_test_issue163.pdf");
    std::fs::write(&tmp_path, &pdf_bytes).unwrap();
    let doc = PdfDocument::open(&tmp_path).unwrap();
    let _ = std::fs::remove_file(&tmp_path);
    assert_eq!(doc.page_count().unwrap(), 1);

    let text = doc.extract_text(0).unwrap();
    assert!(text.is_empty() || text.len() < 100);

    // extract_images should not hang or crash (this was the segfault path) ~keep
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());

    let text = doc.extract_text(0).unwrap();
    drop(text);
}

/// Build a minimal PDF with circular Form XObjects: X0 references X1, X1 references X0.
fn build_circular_xobject_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /X0 5 0 R /X1 6 0 R >> >> >>\nendobj\n");

    let off4 = pdf.len();
    let content = b"/X0 Do";
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    let x0_content = b"/X1 Do";
    pdf.extend_from_slice(format!("5 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << /XObject << /X1 6 0 R >> >> /Length {} >>\nstream\n", x0_content.len()).as_bytes());
    pdf.extend_from_slice(x0_content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off6 = pdf.len();
    let x1_content = b"/X0 Do";
    pdf.extend_from_slice(format!("6 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << /XObject << /X0 5 0 R >> >> /Length {} >>\nstream\n", x1_content.len()).as_bytes());
    pdf.extend_from_slice(x1_content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off6).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

/// Build a minimal one-page PDF whose Form XObject is invoked as
/// `q <6 numbers> /Name Do Q` — deliberately missing the `cm` operator
/// token, so the numbers are dangling operands with nothing to consume
/// them. Per ISO 32000-1:2008 §7.8.2 an operator's operand is whatever
/// immediately precedes it in the stream; `Do`'s operand here is still
/// the Name, not the stray numbers ahead of it.
///
/// `direct_text`: when `Some`, the page's own content stream also draws
/// this text directly before invoking the XObject; when `None`, the page
/// draws nothing itself and all text comes from the XObject.
fn build_xobject_do_with_orphaned_operands_pdf(direct_text: Option<&str>) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] \
              /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> \
              /XObject << /Overlay 6 0 R >> >> >>\nendobj\n",
    );

    let off4 = pdf.len();
    let mut content = Vec::new();
    if let Some(text) = direct_text {
        content.extend_from_slice(format!("BT /F1 12 Tf 1 0 0 1 20 250 Tm ({text}) Tj ET\n").as_bytes());
    }
    // Deliberately missing `cm`: dangling "1 0 0 1 20 150" operands
    // directly precede "/Overlay Do". ~keep
    content.extend_from_slice(b"q 1 0 0 1 20 150 /Overlay Do Q");

    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(&content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
              /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    let off6 = pdf.len();
    let xobj_content = b"BT /F1 12 Tf 1 0 0 1 10 12 Tm (overlay text) Tj ET";
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 300 40] /Length {} >>\nstream\n",
            xobj_content.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(xobj_content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in [off1, off2, off3, off4, off5, off6] {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

/// A page that draws text directly AND paints an overlay Form XObject
/// invoked without a `cm` (dangling operands ahead of the XObject name)
/// must extract both the direct text and the XObject's text, matching
/// poppler's and pymupdf's behaviour on the same malformed content (both
/// tools resolve `Do`'s name from whatever immediately precedes it,
/// discarding the dangling numeric operands rather than misreading them
/// as the XObject name).
#[test]
fn test_direct_and_overlay_xobject_text_both_extracted_with_orphaned_do_operands() {
    let pdf_bytes = build_xobject_do_with_orphaned_operands_pdf(Some("base body text"));
    let doc = PdfDocument::from_bytes(pdf_bytes).expect("parse repro pdf");

    let chars: String = doc.extract_chars(0).unwrap().iter().map(|c| c.char).collect();
    assert!(chars.contains("base body text"), "missing direct text: {chars:?}");
    assert!(chars.contains("overlay text"), "missing XObject text: {chars:?}");

    let text = doc.extract_text(0).unwrap();
    assert!(text.contains("base body text"));
    assert!(text.contains("overlay text"));

    let plain = doc.extract_text(0).unwrap();
    assert!(plain.contains("base body text"));
    assert!(plain.contains("overlay text"));
}

/// A page with no direct content of its own, whose only text comes from
/// a Form XObject invoked without a `cm`, must not extract as empty.
#[test]
fn test_xobject_only_page_text_extracted_with_orphaned_do_operands() {
    let pdf_bytes = build_xobject_do_with_orphaned_operands_pdf(None);
    let doc = PdfDocument::from_bytes(pdf_bytes).expect("parse repro pdf");

    let chars: String = doc.extract_chars(0).unwrap().iter().map(|c| c.char).collect();
    assert_eq!(chars, "overlay text");

    let text = doc.extract_text(0).unwrap();
    assert!(text.contains("overlay text"), "extract_text came back empty: {text:?}");
}

/// Build a minimal one-page PDF where XObject "Outer" invokes a second
/// XObject "Inner" (both via the same malformed missing-`cm` `Do` shape
/// as [`build_xobject_do_with_orphaned_operands_pdf`]), and "Inner" is
/// where the actual text lives.
fn build_nested_xobject_do_with_orphaned_operands_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] \
              /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> \
              /XObject << /Outer 6 0 R >> >> >>\nendobj\n",
    );

    let off4 = pdf.len();
    let content = b"q 1 0 0 1 0 0 /Outer Do Q";
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
              /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    let off6 = pdf.len();
    // "Outer" invokes "Inner" — same malformed missing-`cm` shape, one level deeper. ~keep
    let outer_content = b"q 1 0 0 1 10 10 /Inner Do Q";
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 300 300] \
                  /Resources << /XObject << /Inner 7 0 R >> >> /Length {} >>\nstream\n",
            outer_content.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(outer_content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off7 = pdf.len();
    let inner_content = b"BT /F1 12 Tf 1 0 0 1 10 12 Tm (nested text) Tj ET";
    pdf.extend_from_slice(
        format!(
            "7 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 300 40] /Length {} >>\nstream\n",
            inner_content.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(inner_content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 8\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in [off1, off2, off3, off4, off5, off6, off7] {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

/// Recursion into a Form XObject invoked by another Form XObject, with
/// no `cm` at either level, must still resolve each `Do`'s operand
/// correctly and extract the innermost text.
#[test]
fn test_nested_xobject_text_extracted_with_orphaned_do_operands() {
    let pdf_bytes = build_nested_xobject_do_with_orphaned_operands_pdf();
    let doc = PdfDocument::from_bytes(pdf_bytes).expect("parse repro pdf");

    let text = doc.extract_text(0).unwrap();
    assert!(text.contains("nested text"), "nested XObject text missing: {text:?}");
}

// A corrupt/zero startxref forces full-file xref reconstruction.
// Because reconstruction already scans the whole file for every
// uncompressed object, the document must pre-seed its object-scan cache
// from the reconstructed table — so the first object miss is O(1) instead
// of triggering a SECOND full-file scan (the heavy "first extract_text"
// cost on corrupt-xref polyglot PDFs). ~keep
#[test]
fn test_reconstructed_xref_preseeds_scan_cache() {
    let pdf = b"%PDF-1.4\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Count 0 /Kids [] >>\nendobj\n\
            trailer\n<< /Root 1 0 R /Size 3 >>\n\
            startxref\n0\n%%EOF";
    let doc = PdfDocument::from_bytes(pdf.to_vec()).expect("open corrupt-xref pdf");

    let cache = doc.scanned_object_offsets.lock_or_recover();
    let offsets = cache
        .as_ref()
        .expect("reconstructed xref must pre-seed the scan-offset cache");
    assert!(
        offsets.contains_key(&1) && offsets.contains_key(&2),
        "pre-seeded cache should hold the reconstructed object offsets, got {offsets:?}"
    );
}

/// Build a minimal PDF in memory with given content stream bytes.
/// Returns the raw PDF bytes suitable for `PdfDocument::from_bytes`.
fn build_minimal_pdf(content: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << >> >>\nendobj\n",
    );

    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

/// Build a minimal PDF with a `/Font` resource (needed for `Tf`/`Tj`
/// to resolve glyph widths), used by the NaN-bbox regression test.
/// Build a one-page PDF embedding TWO subsets of the SAME base font -
/// `ABCDEF+Helvetica` and `GHIJKL+Helvetica` - whose font programs differ
/// in size. `big_in_f1` chooses which resource slot carries the larger
/// program, so a caller can show the choice does not depend on the order
/// the fonts are encountered.
///
/// Returns `(pdf_bytes, small_program, big_program)`. The programs are not
/// real TrueType: `FontFile2` is decoded and stored verbatim, never parsed,
/// so distinguishable payloads keep the test on the dedup logic.
fn build_pdf_with_two_font_subsets(big_in_f1: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let small: Vec<u8> = b"SMALL-SUBSET-".iter().cycle().take(64).copied().collect();
    let big: Vec<u8> = b"BIG-SUBSET-".iter().cycle().take(512).copied().collect();
    let (f1_prog, f2_prog) = if big_in_f1 {
        (big.clone(), small.clone())
    } else {
        (small.clone(), big.clone())
    };

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offs: Vec<usize> = Vec::new();

    offs.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offs.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offs.push(pdf.len());
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 4 0 R /F2 7 0 R >> >> >>\nendobj\n",
    );

    // /F1 = ABCDEF+Helvetica, /F2 = GHIJKL+Helvetica. Same canonical base
    // name, so they must dedup to a single entry. ~keep
    for (obj, prefix, desc_obj, file_obj, prog) in [(4, "ABCDEF", 5, 6, &f1_prog), (7, "GHIJKL", 8, 9, &f2_prog)] {
        offs.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "{obj} 0 obj\n<< /Type /Font /Subtype /TrueType /BaseFont /{prefix}+Helvetica \
                     /FontDescriptor {desc_obj} 0 R >>\nendobj\n"
            )
            .as_bytes(),
        );

        offs.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "{desc_obj} 0 obj\n<< /Type /FontDescriptor /FontName /{prefix}+Helvetica \
                     /Flags 32 /FontFile2 {file_obj} 0 R >>\nendobj\n"
            )
            .as_bytes(),
        );

        offs.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "{file_obj} 0 obj\n<< /Length {} /Length1 {} >>\nstream\n",
                prog.len(),
                prog.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(prog);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    // Objects were emitted 1,2,3 then 4,5,6 then 7,8,9 - already in order. ~keep
    let xref_off = pdf.len();
    let total = offs.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offs {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes(),
    );

    (pdf, small, big)
}

/// Two subsets of one base font must dedup to ONE entry, and that entry
/// must be the SAME bytes every time - the bug this fix exists for.
///
/// `get_font_set()` hands the subsets back in `HashMap` order, and each
/// call builds a fresh map whose iteration order is independently seeded,
/// so the old `or_insert` returned a different subset from run to run for
/// one unchanged PDF. Extracting repeatedly makes that flake fatal rather
/// than occasional; swapping which resource slot holds the larger program
/// shows the choice is driven by the total order, not by encounter order.
#[test]
fn embedded_font_subset_choice_is_deterministic() {
    for big_in_f1 in [true, false] {
        let (pdf, small, big) = build_pdf_with_two_font_subsets(big_in_f1);
        let mut previous: Option<Vec<u8>> = None;

        for round in 0..64 {
            let doc = PdfDocument::from_bytes(pdf.clone()).expect("open two-subset pdf");
            let fonts = doc.extract_embedded_fonts().expect("extract fonts");

            assert_eq!(
                fonts.len(),
                1,
                "the two subsets share a base name and must dedup to one entry \
                     (big_in_f1={big_in_f1}, round={round})"
            );
            let (name, bytes) = &fonts[0];
            assert_eq!(name, "Helvetica", "the subset prefix must be stripped");
            assert_eq!(
                bytes, &big,
                "the LARGER subset must win regardless of which slot holds it \
                     (big_in_f1={big_in_f1}, round={round})"
            );
            assert_ne!(bytes, &small);

            if let Some(prev) = &previous {
                assert_eq!(
                    prev, bytes,
                    "repeated extraction of one unchanged PDF must be byte-identical \
                         (big_in_f1={big_in_f1}, round={round})"
                );
            }
            previous = Some(bytes.clone());
        }
    }
}

/// The same guarantee on the variant that also returns the Unicode/width
/// maps: it carries its own copy of the subset choice, so it needs its own
/// guard against regressing back to `or_insert`.
#[test]
fn embedded_font_subset_choice_is_deterministic_with_maps() {
    let (pdf, small, big) = build_pdf_with_two_font_subsets(true);
    let mut previous: Option<Vec<u8>> = None;

    for round in 0..64 {
        let doc = PdfDocument::from_bytes(pdf.clone()).expect("open two-subset pdf");
        let fonts = doc
            .extract_embedded_fonts_with_unicode_maps_and_widths()
            .expect("extract fonts with maps");

        assert_eq!(fonts.len(), 1, "must dedup to one entry (round={round})");
        let (name, bytes, _uni, _widths) = &fonts[0];
        assert_eq!(name, "Helvetica");
        assert_eq!(bytes, &big, "the LARGER subset must win (round={round})");
        assert_ne!(bytes, &small);

        if let Some(prev) = &previous {
            assert_eq!(prev, bytes, "repeated extraction must be stable (round={round})");
        }
        previous = Some(bytes.clone());
    }
}

fn build_minimal_pdf_with_font(content: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    // Deliberately no /MediaBox (and none on /Pages 2 0 R to inherit):
    // `postprocess_spans`'s off-page span filter is skipped entirely
    // when `get_page_media_box` errors, so a page missing /MediaBox
    // is the one path where a NaN bbox component survives to the
    // reading-order sort instead of being silently dropped. ~keep
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R \
              /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
    );

    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
              /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in [off1, off2, off3, off4, off5] {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

/// Repeated `search_page_index()` calls (what `search()`/`search_page()`
/// use internally) must reuse the cached index instead of re-extracting
/// and rebuilding it every time — the whole point of the search index
/// (issue: `search()` re-extracted the full document on every call).
/// Redaction (`erase_region`) changes a page's spans, so it must
/// invalidate the cached index the same way it already invalidates
/// `page_spans_cache`.
#[test]
fn search_index_reused_across_calls_and_invalidated_by_redaction() {
    let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hello World) Tj ET";
    let pdf = build_minimal_pdf_with_font(content);
    let doc = PdfDocument::from_bytes(pdf).expect("open pdf");

    let first = doc.search_page_index(0).expect("build search index");
    let second = doc.search_page_index(0).expect("reuse search index");
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "a second search_page_index() call should hit the cache, not rebuild"
    );

    doc.erase_region(0, crate::geometry::Rect::new(0.0, 0.0, 10.0, 10.0))
        .expect("erase_region");
    let third = doc.search_page_index(0).expect("rebuild after redaction");
    assert!(
        !std::sync::Arc::ptr_eq(&first, &third),
        "erase_region must invalidate the cached search index"
    );
}

/// `clear_search_index()` is the caller-facing escape hatch for dropping
/// the (otherwise unbounded) search index to reclaim memory.
#[test]
fn clear_search_index_forces_rebuild() {
    let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hello World) Tj ET";
    let pdf = build_minimal_pdf_with_font(content);
    let doc = PdfDocument::from_bytes(pdf).expect("open pdf");

    let first = doc.search_page_index(0).expect("build search index");
    doc.clear_search_index();
    let second = doc.search_page_index(0).expect("rebuild after clear");
    assert!(
        !std::sync::Arc::ptr_eq(&first, &second),
        "clear_search_index() should force the next call to rebuild"
    );
}

/// `prepare_search()` must populate the index for every page so a
/// subsequent full-document `search()` sweep hits cache on every page,
/// not just the last few (the failure mode `search()`'s own bounded
/// `page_spans_cache` has for documents with more than 8 pages).
#[test]
fn prepare_search_populates_every_page() {
    let pdf = build_multi_page_pdf(10);
    let doc = PdfDocument::from_bytes(pdf).expect("open pdf");

    doc.prepare_search().expect("prepare_search");
    for page in 0..10 {
        let a = doc.search_page_index(page).expect("indexed page");
        let b = doc.search_page_index(page).expect("still cached");
        assert!(
            std::sync::Arc::ptr_eq(&a, &b),
            "page {page} should already be cached by prepare_search()"
        );
    }
}

/// Regression test: a content stream with a degenerate CTM (`a == 0`)
/// and an oversized `Tm` translation literal. The lexer now clamps an
/// overflowing real literal to a finite value (see
/// `lexer::tests::test_parse_oversized_real_clamps_to_finite`), so this
/// specific literal no longer reaches `TjBuffer::new` as `Infinity` and
/// can no longer turn into NaN via `ctm.a * Infinity`. This test is
/// kept as a regression guard for that class of bug — before the lexer
/// clamp, `f64::from_str` *saturated* the all-digit literal to
/// `f64::INFINITY` rather than erroring, and combined with the zero
/// CTM component this became a NaN `bbox.y` in `TjBuffer::new`,
/// panicking `snap_superscript_baselines`'s index sort — the first
/// span sort that runs on every page, before `postprocess_spans`'s
/// off-page filter (which otherwise silently drops any NaN-bbox span
/// and requires a missing `/MediaBox` to bypass, see
/// `build_minimal_pdf_with_font`) — with the exact signature
/// `smallsort.rs: user-provided comparison function does not
/// correctly implement a total order`.
///
/// 320 glyphs with distinct (sub-point-jittered) Y coordinates —
/// matching real OCR/scanned-text bbox noise, so no two glyphs tie in
/// the sort key — emitted in a riffle-shuffled, far-from-sorted
/// order: a near-sorted input lets Rust's pattern-defeating sort skip
/// the internal total-order consistency check entirely, which is why
/// a naive grid/row-major layout would not have reproduced the
/// original panic even with the same NaN present.
#[test]
fn test_nan_bbox_from_oversized_tm_literal_does_not_panic() {
    let n = 320usize;
    let ys: Vec<f32> = (0..n).map(|i| 750.0 - (i as f32) * 1.37).collect();
    let mid = ys.len() / 2;
    let (a, b) = ys.split_at(mid);
    let mut shuffled: Vec<f32> = Vec::with_capacity(ys.len());
    let (mut ai, mut bi) = (a.iter(), b.iter());
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) => {
                shuffled.push(*x);
                shuffled.push(*y);
            }
            (Some(x), None) => shuffled.push(*x),
            (None, Some(y)) => shuffled.push(*y),
            (None, None) => break,
        }
    }

    let mut content = Vec::new();
    content.extend_from_slice(b"BT\n/F1 10 Tf\n");
    for (i, y) in shuffled.iter().enumerate() {
        if i == 1 {
            // The malicious glyph, isolated under its own degenerate
            // CTM (a = 0) so only this glyph's position collapses
            // via `0.0 * Infinity`; ordinary glyphs are unaffected. ~keep
            let huge = "9".repeat(400) + ".0";
            content.extend_from_slice(b"ET\nq\n0 0 0 1 0 0 cm\nBT\n/F1 10 Tf\n");
            content.extend_from_slice(format!("1 0 0 1 {huge} 400 Tm\n(BOOM) Tj\n").as_bytes());
            content.extend_from_slice(b"ET\nQ\nBT\n/F1 10 Tf\n");
        }
        content.extend_from_slice(format!("1 0 0 1 20 {y} Tm\n(X) Tj\n").as_bytes());
    }
    content.extend_from_slice(b"ET\n");

    let pdf_bytes = build_minimal_pdf_with_font(&content);
    let doc = PdfDocument::from_bytes(pdf_bytes).expect("parse repro pdf");
    let result1 = doc.extract_text(0);
    assert!(
        result1.is_ok(),
        "extract_text panicked or errored on NaN bbox coordinate"
    );
    let result2 = doc.extract_spans(0);
    assert!(
        result2.is_ok(),
        "extract_spans panicked or errored on NaN bbox coordinate"
    );
}

/// Build a minimal PDF with a multi-page structure (given page count).
fn build_multi_page_pdf(page_count: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    let kids_str: String = (0..page_count)
        .map(|i| format!("{} 0 R", i + 3))
        .collect::<Vec<_>>()
        .join(" ");
    let pages_obj = format!(
        "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
        kids_str, page_count
    );
    pdf.extend_from_slice(pages_obj.as_bytes());

    for _i in 0..page_count {
        offsets.push(pdf.len());
        let page_obj = format!(
            "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n",
            offsets.len()
        );
        pdf.extend_from_slice(page_obj.as_bytes());
    }

    let xref_off = pdf.len();
    let total_objs = offsets.len() + 1; // +1 for object 0 ~keep
    pdf.extend_from_slice(format!("xref\n0 {}\n", total_objs).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            total_objs, xref_off
        )
        .as_bytes(),
    );

    pdf
}

#[test]
fn test_from_bytes_minimal_pdf() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.version(), (1, 4));
    assert!(doc.trailer().as_dict().is_some());
}

// catalog() must fall back to scanning indirect objects for
// `/Type /Catalog` when the trailer omits /Root. The public open path
// can't reach this — a /Root-less parsed trailer fails root validation
// and xref reconstruction synthesizes a /Root-bearing trailer before
// catalog() ever runs — so cover find_catalog_by_scan() directly: open a
// valid PDF, then strip /Root from the in-memory trailer and confirm
// catalog() still resolves the Catalog by object scan. ~keep
#[test]
fn test_catalog_recovers_when_trailer_omits_root() {
    let mut doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    assert!(doc.catalog().is_ok());

    // Drop /Root so only the indirect-object scan can find the Catalog. ~keep
    match doc.trailer {
        Object::Dictionary(ref mut d) => {
            d.remove("Root");
            assert!(d.get("Root").is_none());
        }
        _ => panic!("trailer is not a dictionary"),
    }

    let catalog = doc
        .catalog()
        .expect("catalog() must recover the /Type /Catalog object by scan when /Root is absent");
    assert_eq!(
        catalog.as_dict().and_then(|d| d.get("Type")).and_then(|t| t.as_name()),
        Some("Catalog"),
        "find_catalog_by_scan must return the actual Catalog object"
    );
}

#[test]
fn test_from_bytes_invalid_data() {
    let result = PdfDocument::from_bytes(b"not a pdf".to_vec());

    // `parse_header` (lenient mode) never fails on missing `%PDF-` -- it defaults to
    // version 1.4 (line ~19772) -- so the actual rejection happens one step later:
    // `find_xref_offset` (crates/xberg-native-pdf/src/xref.rs) finds no `startxref`
    // keyword anywhere in the 9-byte input and returns `Error::InvalidXref`.
    // `open_from_reader` then tries `reconstruct_xref` as a fallback, which also
    // fails (`RE_OBJ_PATTERN` matches no `N G obj` header at all), so it re-returns
    // the *original* `InvalidXref` error rather than the reconstruction one.
    let error = result.expect_err("9 bytes with no PDF structure at all must be rejected");
    assert!(
        matches!(error, Error::InvalidXref),
        "expected Error::InvalidXref, got: {error:?}"
    );
}

#[test]
fn test_from_bytes_empty() {
    let result = PdfDocument::from_bytes(vec![]);
    assert!(result.is_err());
}

#[test]
fn test_version_accessor() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let (major, minor) = doc.version();
    assert_eq!(major, 1);
    assert_eq!(minor, 4);
}

#[test]
fn test_trailer_accessor() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let trailer = doc.trailer();
    let dict = trailer.as_dict().unwrap();
    assert!(dict.contains_key("Root"));
    assert!(dict.contains_key("Size"));
}

#[test]
fn test_debug_impl() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let debug_str = format!("{:?}", doc);
    assert!(debug_str.contains("PdfDocument"));
    assert!(debug_str.contains("version"));
    assert!(debug_str.contains("(1, 4)"));
}

#[test]
fn test_catalog_returns_dictionary() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let catalog = doc.catalog().unwrap();
    let dict = catalog.as_dict().unwrap();
    assert_eq!(dict.get("Type").unwrap().as_name(), Some("Catalog"));
}

#[test]
fn test_page_count_single_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 1);
}

#[test]
fn test_page_count_multiple_pages() {
    let pdf = build_multi_page_pdf(5);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 5);
}

#[test]
fn test_page_count_zero_pages() {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 0);
}

#[test]
fn test_load_object_from_cache() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj_ref = ObjectRef::new(1, 0);
    let obj1 = doc.load_object(obj_ref).unwrap();
    let obj2 = doc.load_object(obj_ref).unwrap();
    assert_eq!(obj1.as_dict().unwrap().get("Type").unwrap().as_name(), Some("Catalog"));
    assert_eq!(obj2.as_dict().unwrap().get("Type").unwrap().as_name(), Some("Catalog"));
}

#[test]
fn test_load_object_missing_returns_null() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj_ref = ObjectRef::new(999, 0);
    let obj = doc.load_object(obj_ref).unwrap();
    // Per PDF Spec 7.3.10: missing objects treated as Null ~keep
    assert!(matches!(obj, Object::Null));
}

#[test]
fn test_resolve_references_integer() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj = Object::Integer(42);
    let resolved = doc.resolve_references(&obj, 3).unwrap();
    assert_eq!(resolved.as_integer(), Some(42));
}

#[test]
fn test_resolve_references_null() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj = Object::Null;
    let resolved = doc.resolve_references(&obj, 3).unwrap();
    assert!(matches!(resolved, Object::Null));
}

#[test]
fn test_resolve_references_max_depth_zero() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj = Object::Reference(ObjectRef::new(1, 0));
    let resolved = doc.resolve_references(&obj, 0).unwrap();
    assert!(resolved.as_reference().is_some());
}

#[test]
fn test_resolve_references_reference() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj = Object::Reference(ObjectRef::new(1, 0));
    let resolved = doc.resolve_references(&obj, 3).unwrap();
    assert!(resolved.as_dict().is_some());
}

#[test]
fn test_resolve_references_array() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![Object::Integer(1), Object::Integer(2)]);
    let resolved = doc.resolve_references(&arr, 3).unwrap();
    let resolved_arr = resolved.as_array().unwrap();
    assert_eq!(resolved_arr.len(), 2);
}

#[test]
fn test_resolve_references_dictionary() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let mut dict = std::collections::HashMap::new();
    dict.insert("Key".to_string(), Object::Integer(42));
    let obj = Object::Dictionary(dict);
    let resolved = doc.resolve_references(&obj, 3).unwrap();
    let resolved_dict = resolved.as_dict().unwrap();
    assert_eq!(resolved_dict.get("Key").unwrap().as_integer(), Some(42));
}

#[test]
fn test_resolve_references_bad_reference() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let obj = Object::Reference(ObjectRef::new(999, 0));
    let resolved = doc.resolve_references(&obj, 3).unwrap();
    assert!(matches!(resolved, Object::Null));
}

#[test]
fn test_authenticate_unencrypted_pdf() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc.authenticate(b"anypassword").unwrap();
    assert!(result);
}

#[test]
fn test_get_page_content_data_empty_content() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let data = doc.get_page_content_data(0).unwrap();
    // Empty content stream still returns data (may be empty or have a newline) ~keep
    assert!(data.len() <= 2);
}

#[test]
fn test_get_page_content_data_with_content() {
    let content = b"BT /F1 12 Tf (Hello) Tj ET";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let data = doc.get_page_content_data(0).unwrap();
    assert!(!data.is_empty());
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("Hello"));
}

#[test]
fn test_get_page_content_data_blank_page() {
    let pdf = build_multi_page_pdf(1);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let data = doc.get_page_content_data(0).unwrap();
    assert!(data.is_empty());
}

#[test]
fn test_extract_text_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_text(0).unwrap();
    assert!(text.is_empty());
}

#[test]
fn test_extract_text_no_font_resources() {
    let content = b"BT /F1 12 Tf (Hello) Tj ET";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    // Should not crash, may return empty or partial text ~keep
    let _text = doc.extract_text(0).unwrap();
}

#[test]
fn test_extract_all_text_multiple_pages() {
    let pdf = build_multi_page_pdf(3);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_all_text().unwrap();
    let page_count = text.matches('\x0c').count();
    assert_eq!(page_count, 2);
}

#[test]
fn test_extract_all_text_single_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_all_text().unwrap();
    assert!(!text.contains('\x0c'));
}

#[test]
fn test_extract_spans_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let spans = doc.extract_spans(0).unwrap();
    assert!(spans.is_empty());
}

#[test]
fn test_extract_spans_no_text_operators() {
    let content = b"100 200 300 400 re S";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let spans = doc.extract_spans(0).unwrap();
    assert!(spans.is_empty());
}

#[test]
fn test_extract_chars_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let chars = doc.extract_chars(0).unwrap();
    assert!(chars.is_empty());
}

#[test]
fn test_may_contain_text_with_bt() {
    let data = b"q BT /F1 12 Tf (Hello) Tj ET Q";
    assert!(PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_with_do() {
    let data = b"q /Im0 Do Q";
    assert!(PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_no_text_operators() {
    let data = b"100 200 300 400 re S";
    assert!(!PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_empty() {
    let data = b"";
    assert!(!PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_bt_at_start() {
    let data = b"BT /F1 12 Tf ET";
    assert!(PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_bt_at_end() {
    let data = b"q Q BT";
    assert!(PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_false_positive_btype() {
    // "BTerror" should not match BT (BT must be delimited) ~keep
    let data = b"BTerror";
    assert!(!PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_false_positive_document() {
    // "Document" contains "Do" but not as a standalone operator ~keep
    let data = b"Document";
    assert!(!PdfDocument::may_contain_text(data));
}

#[test]
fn test_may_contain_text_do_with_name() {
    let data = b"/Im0 Do\n";
    assert!(PdfDocument::may_contain_text(data));
}

/// Helper to create a TextSpan with minimal required fields for testing.
#[test]
fn test_try_assemble_vertical_cjk_orders_columns_right_to_left() {
    // Three columns of CJK glyphs; the right column (x=116) is read first,
    // top-to-bottom, then the next column to the left, etc. ~keep
    let mk = |t: &str, x: f32, y: f32| make_test_span(t, x, y, 18.0, 18.0);
    let spans = vec![
        mk("\u{4E00}", 116.0, 719.0),
        mk("\u{4E8C}", 116.0, 701.0),
        mk("\u{4E09}", 116.0, 683.0),
        mk("\u{56DB}", 89.0, 719.0),
        mk("\u{4E94}", 89.0, 701.0),
        mk("\u{516D}", 89.0, 683.0),
        mk("\u{4E03}", 62.0, 719.0),
        mk("\u{516B}", 62.0, 701.0),
        mk("\u{4E5D}", 62.0, 683.0),
    ];
    assert_eq!(
        PdfDocument::try_assemble_vertical_cjk(&spans).as_deref(),
        Some("\u{4E00}\u{4E8C}\u{4E09}\u{56DB}\u{4E94}\u{516D}\u{4E03}\u{516B}\u{4E5D}")
    );
}

#[test]
fn test_try_assemble_vertical_cjk_horizontal_returns_none() {
    // A horizontal CJK row (glyphs advance in X at a fixed Y) must NOT be
    // detected as vertical — horizontal documents stay on the normal path. ~keep
    let mk = |t: &str, x: f32| make_test_span(t, x, 700.0, 18.0, 18.0);
    let spans = vec![
        mk("\u{4E00}", 62.0),
        mk("\u{4E8C}", 80.0),
        mk("\u{4E09}", 98.0),
        mk("\u{56DB}", 116.0),
        mk("\u{4E94}", 134.0),
        mk("\u{516D}", 152.0),
        mk("\u{4E03}", 170.0),
        mk("\u{516B}", 188.0),
    ];
    assert!(PdfDocument::try_assemble_vertical_cjk(&spans).is_none());
}

#[test]
fn test_try_assemble_vertical_cjk_multichar_runs_returns_none() {
    // Horizontal CJK emitted as multi-character RUNS (a whole line per show
    // op), stacked top-to-bottom. Each run's nearest neighbour is the run on
    // the line above/below (vertical) — but these are horizontal lines, not
    // tategaki columns. The single-glyph-span gate must keep this on the
    // horizontal path so the reading order is not shredded. ~keep
    let mk = |t: &str, y: f32| make_test_span(t, 60.0, y, 200.0, 18.0);
    let spans = vec![
        mk("標準マーケットモデルは", 700.0),
        mk("次元で説明することは", 680.0),
        mk("取引できない商品がマ", 660.0),
        mk("クロ経済変数などである", 640.0),
        mk("モデルを考えることは", 620.0),
        mk("可能で非完備の場合は", 600.0),
        mk("リスク中立確率は一意", 580.0),
        mk("ではなく価格も一意でない", 560.0),
    ];
    assert!(PdfDocument::try_assemble_vertical_cjk(&spans).is_none());
}

#[test]
fn test_try_assemble_vertical_cjk_latin_returns_none() {
    // A Latin page is not CJK-majority → None (never vertical). ~keep
    let spans: Vec<TextSpan> = "the quick brown fox jumps over a lazy dog today"
        .split(' ')
        .enumerate()
        .map(|(i, w)| make_test_span(w, 60.0 + i as f32 * 40.0, 700.0, 30.0, 12.0))
        .collect();
    assert!(PdfDocument::try_assemble_vertical_cjk(&spans).is_none());
}

#[test]
fn test_push_line_breaks_table_row_single_newline() {
    // A table-row boundary (single_break = true) emits exactly one newline
    // regardless of the geometric row pitch. ~keep
    let prev = make_test_span("North", 72.0, 700.0, 30.0, 12.0);
    let span = make_test_span("South", 72.0, 676.0, 30.0, 12.0); // 24pt gap ≈ 1.7em ~keep
    let mut out = String::new();
    PdfDocument::push_line_breaks(&mut out, &prev, &span, 24.0, true);
    assert_eq!(out, "\n", "table row boundary must be a single newline");
    // The same gap WITHOUT the table flag rounds to a blank line (2). ~keep
    let mut out2 = String::new();
    PdfDocument::push_line_breaks(&mut out2, &prev, &span, 24.0, false);
    assert_eq!(out2, "\n\n", "non-table ~1.7em gap keeps the geometric blank line");
    // A single-line gap stays one newline either way. ~keep
    let mut out3 = String::new();
    PdfDocument::push_line_breaks(&mut out3, &prev, &span, 14.0, false);
    assert_eq!(out3, "\n");
}

fn make_test_span(text: &str, x: f32, y: f32, width: f32, font_size: f32) -> TextSpan {
    TextSpan {
        provenance: None,
        text_rise: 0.0,
        artifact_type: None,
        text: text.to_string(),
        bbox: crate::geometry::Rect {
            x,
            y,
            width,
            height: font_size,
        },
        font_name: "F1".to_string(),
        font_size,
        font_weight: crate::layout::FontWeight::Normal,
        is_italic: false,
        is_monospace: false,
        color: crate::layout::Color::new(0.0, 0.0, 0.0),
        mcid: None,
        mcid_scope: None,
        sequence: 0,
        split_boundary_before: false,
        offset_semantic: false,
        char_spacing: 0.0,
        word_spacing: 0.0,
        horizontal_scaling: 100.0,
        primary_detected: false,
        char_widths: vec![],
        char_x_offsets: Vec::new(),
        heading_level: None,
        rotation_degrees: 0.0,
        wmode: 0,
        rtl_draw_logical: false,
        mirrored: false,
        page_rotation_applied: 0,
    }
}

#[test]
fn test_should_insert_space_same_line_with_gap() {
    let prev = make_test_span("Hello", 0.0, 100.0, 50.0, 12.0);
    let current = make_test_span("World", 56.0, 100.0, 50.0, 12.0);
    // 6pt gap (> 0.25 * 12 = 3pt) ~keep
    assert!(PdfDocument::should_insert_space(&prev, &current));
}

/// A single word drawn as two same-font runs with real (varying) per-glyph
/// metrics that overlap by a fraction of a point ("PLANAL"+"TINA", the
/// planaltina kerning-split repro) is a reliable kerning overlap: the
/// assembler must NOT insert a space, reconstructing "PLANALTINA".
#[test]
fn test_reliable_kerning_overlap_recognizes_split_word() {
    let mut prev = make_test_span("PLANAL", 100.0, 700.0, 38.35, 10.0);
    prev.char_widths = vec![6.67, 5.56, 6.67, 7.22, 6.67, 5.56]; // real Helvetica ~keep
    let span = make_test_span("TINA", 136.64, 700.0, 22.78, 10.0);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width); // ≈ -1.71pt overlap ~keep
    assert!(
        PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap),
        "varying-width same-font runs overlapping by <1em must read as one word"
    );
}

/// A font with no /Widths array falls back to a uniform advance per glyph,
/// over-reporting each width and manufacturing a fake overlap between two
/// SEPARATE words ("STATION"+"FREEDOM"). Uniform char_widths must NOT be
/// treated as a kerning overlap — the assembler keeps the word-boundary
/// space.
#[test]
fn test_reliable_kerning_overlap_rejects_uniform_fallback_widths() {
    let mut prev = make_test_span("STATION", 100.0, 700.0, 42.0, 10.0);
    prev.char_widths = vec![6.0; 7]; // uniform missing-/Widths fallback ~keep
    let span = make_test_span("FREEDOM", 141.0, 700.0, 42.0, 10.0);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width); // -1.0pt fake overlap ~keep
    assert!(
        !PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap),
        "uniform fallback widths are an inflated-width artifact, not kerning"
    );
}

/// A coarse width table with only two distinct advances (e.g. a font that
/// reports one width for wide glyphs and one for narrow) is not genuine
/// proportional metrics — it manufactures fake overlaps between SEPARATE
/// words ("território"+"e"). Two distinct advances must NOT qualify.
#[test]
fn test_reliable_kerning_overlap_rejects_coarse_two_value_widths() {
    let mut prev = make_test_span("território", 32.0, 700.0, 68.0, 13.6);
    prev.char_widths = vec![6.8, 6.8, 6.8, 6.8, 6.8, 6.8, 3.4, 3.4, 6.8, 6.8];
    let span = make_test_span("e", 66.0, 700.0, 6.8, 13.6);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width);
    assert!(!PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap));
}

/// A lowercase→uppercase transition at an overlapping join is a
/// word/sentence boundary ("...with"+"Gp53"), not one word split by
/// kerning — it must NOT be treated as a reliable kerning overlap even with
/// real varying widths.
#[test]
fn test_reliable_kerning_overlap_rejects_lowercase_to_uppercase_boundary() {
    let mut prev = make_test_span("with", 100.0, 700.0, 20.0, 12.0);
    prev.char_widths = vec![6.7, 3.3, 4.8, 6.7];
    let span = make_test_span("Gp53", 118.0, 700.0, 24.0, 12.0);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width); // -2pt overlap ~keep
    assert!(!PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap));
}

/// A positive gap is a genuine word boundary, never a kerning overlap.
#[test]
fn test_reliable_kerning_overlap_requires_negative_gap() {
    let mut prev = make_test_span("Hello", 0.0, 100.0, 28.0, 10.0);
    prev.char_widths = vec![6.0, 3.0, 3.0, 3.0, 6.0];
    let span = make_test_span("World", 32.0, 100.0, 28.0, 10.0);
    let gap = span.bbox.x - (prev.bbox.x + prev.bbox.width); // +4pt gap ~keep
    assert!(!PdfDocument::is_reliable_kerning_overlap(&prev, &span, gap));
}

// --- topological_block_order (multi-region reading order) -----------------
// Larger Y = higher on the page (read first). Two columns separated by a
// gutter (a > ~1 em horizontal gap) must be read column-major (left column
// fully, then right), and only genuine multi-column PROSE should engage it —
// single-column, fragmented tables and TOC page-number rails must be left on
// the row-aware path (return None) so their output is unchanged. ~keep

fn two_dense_columns(left_dense: bool, right_dense: bool) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    for k in 0..8 {
        let y = 200.0 - k as f32 * 12.0;
        let l = if left_dense {
            format!("left column body sentence number {k} here")
        } else {
            format!("{k}")
        };
        let r = if right_dense {
            format!("right column body sentence number {k} here")
        } else {
            format!("{}", (k + 1) * 10) // short page-number-like values ~keep
        };
        spans.push(make_test_span(&l, 0.0, y, 90.0, 10.0));
        spans.push(make_test_span(&r, 120.0, y, 90.0, 10.0));
    }
    spans
}

#[test]
fn topo_two_column_prose_reads_column_major() {
    let spans = two_dense_columns(true, true);
    let out = PdfDocument::topological_block_order(&spans).expect("genuine two-column prose must be reordered");
    assert_eq!(out.len(), spans.len());
    let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
    let last_left = texts.iter().rposition(|t| t.starts_with("left")).unwrap();
    let first_right = texts.iter().position(|t| t.starts_with("right")).unwrap();
    // Whole left column before whole right column (de-interleaved). ~keep
    assert!(last_left < first_right, "columns interleaved: {texts:?}");
}

#[test]
fn topo_tight_leading_two_columns_stay_separate() {
    // Two dense columns whose gutter (18 pt) is NARROWER than med_h (font
    // size 20): the same-row gap (18) is below med_h*1.0 (20), so WITHOUT the
    // Item 4 gutter veto the union-find fuses left+right into one block and
    // the side_by_side gate then declines → None → row-major interleave. With
    // the veto the two columns stay separate and read column-major. ~keep
    let mut spans = Vec::new();
    for k in 0..8 {
        let y = 200.0 - k as f32 * 24.0;
        spans.push(make_test_span(
            &format!("left column body sentence number {k} here"),
            0.0,
            y,
            90.0,
            20.0,
        ));
        spans.push(make_test_span(
            &format!("right column body sentence number {k} here"),
            108.0,
            y,
            90.0,
            20.0,
        )); // gutter 108-90 = 18 (< med_h 20) ~keep
    }
    let out =
        PdfDocument::topological_block_order(&spans).expect("tight-gutter two columns must stay separate and reorder");
    let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
    let last_left = texts.iter().rposition(|t| t.starts_with("left")).unwrap();
    let first_right = texts.iter().position(|t| t.starts_with("right")).unwrap();
    assert!(last_left < first_right, "tight-gutter columns fused: {texts:?}");
}

#[test]
fn topo_single_column_returns_none() {
    let mut spans = Vec::new();
    for k in 0..10 {
        let y = 200.0 - k as f32 * 12.0;
        spans.push(make_test_span(
            &format!("single column body line {k} of running text"),
            0.0,
            y,
            190.0,
            10.0,
        ));
    }
    // No side-by-side region → unchanged (row-aware path). ~keep
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

#[test]
fn topo_toc_sparse_page_number_column_returns_none() {
    // Left = chapter titles (dense), right = page numbers (sparse). The
    // text-density gate must reject it so a TOC is not read column-major
    // (which would divorce each title from its page number). ~keep
    let spans = two_dense_columns(true, false);
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

#[test]
fn topo_fragmented_table_returns_none() {
    // Two dense columns PLUS many isolated single-token fragment blocks
    // (page numbers / cell labels) the union-find cannot coalesce — the
    // signature of a structured table (chess diagram, data grid), which must
    // stay row-aware rather than be read column-major. ~keep
    let mut spans = two_dense_columns(true, true);
    for k in 0..10 {
        // Widely scattered short tokens → each its own fragment block. ~keep
        let x = 230.0 + (k as f32) * 30.0;
        let y = 205.0 - (k as f32) * 17.0;
        spans.push(make_test_span(&format!("{}", k * 7), x, y, 8.0, 10.0));
    }
    assert!(PdfDocument::topological_block_order(&spans).is_none());
}

#[test]
fn test_y_band_candidates_is_superset_of_tolerance() {
    let band = 4.0_f32;
    let spans: Vec<TextSpan> = [0.0, 1.5, 3.9, 4.0, 4.1, 8.0, 100.0, -3.0, -8.0]
        .iter()
        .map(|&y| make_test_span("x", 0.0, y, 5.0, 10.0))
        .collect();
    let idx = PdfDocument::build_y_band_index(&spans, band);
    for &cy in &[0.0_f32, 4.0, 4.05, 100.0, -3.0] {
        let got: std::collections::HashSet<usize> = PdfDocument::y_band_candidates(&idx, cy, band).collect();
        for (j, s) in spans.iter().enumerate() {
            if (s.bbox.y - cy).abs() <= band {
                assert!(
                    got.contains(&j),
                    "index missed span {j} (y={}) within band of cy={cy}",
                    s.bbox.y
                );
            }
        }
    }
}

#[test]
fn test_merge_drop_cap_initial() {
    let mut spans = vec![
        make_test_span("T", 0.0, 100.0, 14.0, 20.0),
        make_test_span("ABLE 102.3", 15.0, 100.0, 60.0, 12.0),
    ];
    PdfDocument::merge_drop_cap_initials(&mut spans);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "TABLE 102.3");
    assert_eq!(spans[0].bbox.x, 0.0);
}

#[test]
fn test_merge_drop_cap_skips_same_size_capital() {
    let mut spans = vec![
        make_test_span("A", 0.0, 100.0, 8.0, 12.0),
        make_test_span("word", 9.0, 100.0, 30.0, 12.0),
    ];
    PdfDocument::merge_drop_cap_initials(&mut spans);
    assert_eq!(spans.len(), 2, "same-size capital is not a drop-cap initial");
}

#[test]
fn test_merge_drop_cap_skips_math_subscript_base() {
    let mut spans = vec![
        make_test_span("the shuffle algebra", 0.0, 100.0, 90.0, 10.0),
        make_test_span("A", 92.0, 100.0, 7.0, 10.0),
        make_test_span("st", 99.0, 98.0, 6.0, 6.5),
        make_test_span("of a statistic", 106.0, 100.0, 70.0, 10.0),
    ];
    PdfDocument::merge_drop_cap_initials(&mut spans);
    assert_eq!(spans.len(), 4, "inline math base letter is not a drop-cap initial");
    assert_eq!(spans[1].text, "A");
    assert_eq!(spans[2].text, "st");
}

#[test]
fn test_merge_drop_cap_skips_word_spaced_standalone_capital() {
    let mut spans = vec![
        make_test_span("ordinary body sentence one", 0.0, 200.0, 120.0, 10.0),
        make_test_span("ordinary body sentence two", 0.0, 188.0, 120.0, 10.0),
        make_test_span("A", 0.0, 100.0, 12.0, 18.0),
        make_test_span("Perspective", 17.0, 100.0, 90.0, 18.0),
    ];
    PdfDocument::merge_drop_cap_initials(&mut spans);
    assert_eq!(spans.len(), 4, "word-spaced standalone capital is not a drop cap");
    assert_eq!(spans[2].text, "A");
    assert_eq!(spans[3].text, "Perspective");
}

#[test]
fn merge_sub_superscript_accepts_fstatistic() {
    let mut spans = vec![
        make_test_span("F", 0.0, 100.0, 8.0, 12.0),
        make_test_span("4,176", 8.0, 98.0, 12.0, 8.0),
    ];
    PdfDocument::merge_sub_superscript_spans(&mut spans);
    assert_eq!(spans.len(), 1, "index cluster must merge into base");
    assert_eq!(spans[0].text, "F4,176");
}

#[test]
fn merge_sub_superscript_accepts_text_rise_flagged() {
    let mut base = make_test_span("M", 0.0, 100.0, 10.0, 12.0);
    base.text_rise = 0.0;
    let mut sup = make_test_span("\u{22C6}", 10.5, 103.0, 6.0, 12.0);
    sup.text_rise = 0.30;
    let mut spans = vec![base, sup];
    PdfDocument::merge_sub_superscript_spans(&mut spans);
    assert_eq!(spans.len(), 1, "Ts-flagged superscript must merge into base");
    assert_eq!(spans[0].text, "M\u{22C6}");
}

#[test]
fn merge_sub_superscript_accepts_same_baseline_numeric() {
    let base = make_test_span("[", 0.0, 100.0, 6.0, 18.0);
    let sup = make_test_span("123", 6.0, 100.0, 20.0, 13.0);
    let mut spans = vec![base, sup];
    PdfDocument::merge_sub_superscript_spans(&mut spans);
    assert_eq!(spans.len(), 1, "same-baseline numeric superscript must merge");
    assert_eq!(spans[0].text, "[123");
}

#[test]
fn merge_sub_superscript_rejects_same_baseline_alpha() {
    let base = make_test_span("A", 0.0, 100.0, 12.0, 18.0);
    let sub = make_test_span("bc", 12.0, 100.0, 8.0, 13.0);
    let mut spans = vec![base, sub];
    PdfDocument::merge_sub_superscript_spans(&mut spans);
    assert_eq!(spans.len(), 2, "same-baseline alpha run must not merge");
}

#[test]
fn merge_sub_superscript_keeps_table_number_separate() {
    // Guard: a bare figure/table number after a WORD base is not an index
    // cluster (no comma) and the word base is invalid — stays separate. ~keep
    let mut spans = vec![
        make_test_span("Table", 0.0, 100.0, 30.0, 12.0),
        make_test_span("3", 31.0, 100.0, 6.0, 12.0),
    ];
    PdfDocument::merge_sub_superscript_spans(&mut spans);
    assert_eq!(spans.len(), 2, "Table 3 must not merge");
}

#[test]
fn test_order_rotated_blocks_groups_by_rotation() {
    let mk = |t: &str, x: f32, y: f32, rot: f32| {
        let mut s = make_test_span(t, x, y, 10.0, 10.0);
        s.rotation_degrees = rot;
        s
    };
    // Two 90° runs (seen first) then one -90° run. ~keep
    let spans = vec![
        mk("A", 10.0, 50.0, 90.0),
        mk("B", 10.0, 80.0, 90.0),
        mk("C", 200.0, 50.0, -90.0),
    ];
    let out = PdfDocument::order_rotated_blocks(spans);
    assert_eq!(out.len(), 3, "no spans dropped");
    // Groups stay contiguous in first-seen order; 90° block before -90°. ~keep
    let rots: Vec<f32> = out.iter().map(|s| s.rotation_degrees).collect();
    assert_eq!(rots, vec![90.0, 90.0, -90.0]);
    // Within the 90° block, upright-frame order keeps A before B. ~keep
    assert_eq!(out[0].text, "A");
    assert_eq!(out[1].text, "B");
}

#[test]
fn test_merge_drop_cap_does_not_reach_line_above() {
    // A tall oversized "A" (16.8pt, baseline y=328) whose bbox top reaches
    // up into the previous line ("Or if", y~342). It must NOT merge with
    // "if" on the line above — only with same-baseline words on its own line
    // (which here are word-spaced and so also stay separate). Reproduces the
    // alice_old "OrAif" corruption. ~keep
    let mut spans = vec![
        make_test_span("Or", 44.0, 344.0, 14.4, 12.0),
        make_test_span("if", 62.0, 342.5, 10.7, 8.9),
        make_test_span("Idrop upon my toe", 74.0, 343.9, 90.0, 12.2),
        make_test_span("A", 54.7, 328.1, 10.1, 16.8),
        make_test_span("very heavy weight", 69.8, 327.8, 90.0, 8.4),
    ];
    PdfDocument::merge_drop_cap_initials(&mut spans);
    assert!(
        spans.iter().all(|s| s.text != "Aif" && !s.text.contains("OrA")),
        "tall initial must not steal a word from the line above"
    );
    assert!(
        spans.iter().any(|s| s.text == "A"),
        "initial left intact on its own line"
    );
}

#[test]
fn test_should_insert_space_same_line_no_gap() {
    let prev = make_test_span("Hello", 0.0, 100.0, 50.0, 12.0);
    let current = make_test_span("World", 51.0, 100.0, 50.0, 12.0);
    // 1pt gap (< 0.25 * 12 = 3pt) ~keep
    assert!(!PdfDocument::should_insert_space(&prev, &current));
}

#[test]
fn test_should_insert_space_different_lines() {
    let prev = make_test_span("Hello", 0.0, 100.0, 50.0, 12.0);
    let current = make_test_span("World", 56.0, 120.0, 50.0, 12.0);
    // Different lines = false (no space needed, line break instead) ~keep
    assert!(!PdfDocument::should_insert_space(&prev, &current));
}

#[test]
fn test_should_insert_space_column_gap() {
    let prev = make_test_span("Hello", 0.0, 100.0, 50.0, 12.0);
    let current = make_test_span("World", 200.0, 100.0, 50.0, 12.0);
    // Issue 487 (pr-138-example.pdf rate tables): a very large
    // same-line gap (here 150 pt > 5 em) must still produce a single
    // space. The earlier `gap < font_size * 5.0` upper bound made
    // this return false, after which the caller concatenated the two
    // spans without a separator and `3.80%` + `4.41%` came out as
    // `3.80%4.41%`. Large gap = different column = still a space. ~keep
    assert!(PdfDocument::should_insert_space(&prev, &current));
}

/// Stacked two-line column/table-header cell: `Comparison` drawn over
/// `rate` at a baseline drop that stays just under `same_line_threshold`,
/// so the caller treats them as one line and defers here. The two spans
/// horizontally OVERLAP (negative gap), which the positive-gap test would
/// reject — fusing them into `Comparisonrate`. A negative gap combined with
/// a real baseline shift is two stacked tokens (never intra-word kerning,
/// which shares a baseline), so a space must be inserted.
#[test]
fn test_stacked_cell_needs_space_overlapping_rows() {
    // fs=12 → same_line_threshold = max(14.4, 3.6) = 14.4; y_diff = 8 stays
    // under it (one line), gap = 20 - 60 = -40 (overlap). ~keep
    let upper = make_test_span("Comparison", 0.0, 108.0, 60.0, 12.0);
    let lower = make_test_span("rate", 20.0, 100.0, 25.0, 12.0);
    assert!(
        PdfDocument::stacked_cell_needs_space(&upper, &lower),
        "stacked overlapping cells with a baseline shift must be separated by a space"
    );
}

/// Guard: two spans on the SAME baseline that overlap by a couple points
/// (real intra-word kerning, e.g. `eigen`+`value` split by a font's tight
/// side-bearings) must NOT be flagged — the baseline shift is what
/// distinguishes a stacked cell from kerning.
#[test]
fn test_stacked_cell_same_baseline_overlap_is_kerning() {
    let prev = make_test_span("eigen", 0.0, 100.0, 30.0, 12.0);
    let current = make_test_span("value", 28.0, 100.0, 30.0, 12.0);
    assert!(
        !PdfDocument::stacked_cell_needs_space(&prev, &current),
        "same-baseline overlap is intra-word kerning, not a word boundary"
    );
}

/// Two glyphs of the same complex Brahmic script with an intra-word
/// gap (a Bengali matra-cluster `ছো` followed by `ট`, ~9pt apart at 13pt)
/// must NOT get a heuristic space — word breaks in these scripts are
/// carried by explicit SPACE glyphs (§14.8.2.5), and the Latin-tuned gap
/// test otherwise splits syllables (`ছো ট`). Mirrors the CJK guard.
#[test]
fn test_should_insert_space_suppressed_within_complex_script() {
    // Bengali: prev ends in matra ো (U+09CB), next is consonant ট (U+09AF…
    // here U+099F) — same script, ~9pt gap. ~keep
    let prev = make_test_span("\u{099B}\u{09CB}", 0.0, 100.0, 7.0, 13.0);
    let current = make_test_span("\u{099F}", 16.0, 100.0, 5.0, 13.0);
    assert!(
        !PdfDocument::should_insert_space(&prev, &current),
        "intra-word complex-script gap must not insert a space"
    );
    // Tamil likewise (prev ends in vowel sign ை U+0BC8, next consonant). ~keep
    let p2 = make_test_span("\u{0B87}\u{0BA9}\u{0BC8}", 0.0, 100.0, 20.0, 13.0);
    let c2 = make_test_span("\u{0B9A}\u{0BCD}", 30.0, 100.0, 8.0, 13.0);
    assert!(!PdfDocument::should_insert_space(&p2, &c2));
}

/// The guard is script-specific: a complex-script glyph meeting a Latin
/// glyph across a real gap still gets its boundary space (only *same*-script
/// intra-word gaps are suppressed).
#[test]
fn test_should_insert_space_kept_across_script_boundary() {
    let prev = make_test_span("\u{0B95}", 0.0, 100.0, 12.0, 13.0);
    let current = make_test_span("A", 18.0, 100.0, 8.0, 13.0);
    assert!(
        PdfDocument::should_insert_space(&prev, &current),
        "complex↔Latin boundary gap must keep its space"
    );
}

fn make_decimal_span(text: &str, char_widths: Vec<f32>, bbox_w: f32, font_size: f32) -> TextSpan {
    TextSpan {
        provenance: None,
        text_rise: 0.0,
        text: text.to_string(),
        bbox: crate::geometry::Rect {
            x: 0.0,
            y: 0.0,
            width: bbox_w,
            height: font_size,
        },
        font_name: "F1".to_string(),
        font_size,
        font_weight: crate::layout::FontWeight::Normal,
        is_italic: false,
        is_monospace: false,
        color: crate::layout::Color::new(0.0, 0.0, 0.0),
        mcid: None,
        mcid_scope: None,
        sequence: 0,
        split_boundary_before: false,
        offset_semantic: false,
        char_spacing: 0.0,
        word_spacing: 0.0,
        horizontal_scaling: 100.0,
        primary_detected: false,
        artifact_type: None,
        char_widths,
        char_x_offsets: Vec::new(),
        heading_level: None,
        rotation_degrees: 0.0,
        wmode: 0,
        rtl_draw_logical: false,
        mirrored: false,
        page_rotation_applied: 0,
    }
}

#[test]
fn test_column_spanning_decimal_wide_bbox() {
    // "1.10": 4 chars, cw=[3.98], expected=15.92, gap=9.8 > fs(7.0) → split ~keep
    let span = make_decimal_span("1.10", vec![3.9811199], 25.72, 7.0);
    assert!(PdfDocument::is_column_spanning_decimal(&span));
}

#[test]
fn test_column_spanning_decimal_5char_span() {
    // "12.11": 5 chars, cw=[3.98,3.98], expected=19.91, gap=7.73 > fs(7.0) → split ~keep
    let span = make_decimal_span("12.11", vec![3.9811199, 3.9811199], 27.64, 7.0);
    assert!(PdfDocument::is_column_spanning_decimal(&span));
}

#[test]
fn test_column_spanning_decimal_normal_bbox() {
    // "1.5" with 3 entries matching 3 chars; bbox_w = expected → gap ≈ 0 → no split ~keep
    let span = make_decimal_span("1.5", vec![3.0, 3.0, 3.0], 9.0, 7.0);
    assert!(!PdfDocument::is_column_spanning_decimal(&span));
}

#[test]
fn test_column_spanning_decimal_non_digit() {
    // "hello.world" — letters, not digits → no split ~keep
    let span = make_decimal_span("hello.world", vec![], 60.0, 12.0);
    assert!(!PdfDocument::is_column_spanning_decimal(&span));
}

#[test]
fn test_column_spanning_decimal_multiple_dots() {
    // "1.2.3" — two dots → no split ~keep
    let span = make_decimal_span("1.2.3", vec![3.0], 25.0, 7.0);
    assert!(!PdfDocument::is_column_spanning_decimal(&span));
}

#[test]
fn test_push_span_text_splits_wide_decimal() {
    let span = make_decimal_span("1.10", vec![3.9811199], 25.72, 7.0);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "1 10");
}

#[test]
fn test_push_span_text_leaves_normal_decimal() {
    let span = make_decimal_span("3.14", vec![4.0, 4.0, 4.0, 4.0], 16.0, 12.0);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "3.14");
}

#[test]
fn test_push_span_text_strips_soft_hyphen_mid_word() {
    // ISO 32000-1 §14.8.2.2.3: U+00AD marks a discretionary line-break
    // point only — it must never survive into extract_text/to_markdown/
    // to_html output, even mid-word with no adjacent line break (the
    // span was drawn as a single reflowed run, not split across lines). ~keep
    let span = make_decimal_span("recon\u{00AD}struction", vec![], 80.0, 12.0);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "reconstruction");
}

#[test]
fn test_push_span_text_strips_multiple_soft_hyphens() {
    let span = make_decimal_span("un\u{00AD}be\u{00AD}liev\u{00AD}able", vec![], 100.0, 12.0);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "unbelievable");
}

#[test]
fn test_cw_boundary_split_theorem_number() {
    // "Theorem1.7": 10 chars, 7 widths → split before '1' ~keep
    let span = make_decimal_span("Theorem1.7", vec![11.2, 8.9, 7.4, 8.1, 6.6, 7.4, 13.4], 83.8, 14.3);
    let result = PdfDocument::char_widths_boundary_split(&span);
    assert_eq!(result, Some(7)); // byte 7 = '1' ~keep
}

#[test]
fn test_cw_boundary_split_let_capital() {
    // "LetC": 4 chars, 3 widths — lower→upper boundary → split at 'C'
    // (represents two CID text runs "Let" + "C" concatenated) ~keep
    let span = make_decimal_span("LetC", vec![7.3, 5.2, 4.5], 26.7, 12.0);
    let result = PdfDocument::char_widths_boundary_split(&span);
    assert_eq!(result, Some(3)); // byte 3 = 'C' ~keep
}

#[test]
fn test_cw_boundary_no_split_already_space() {
    // "Theorem 1.1": 7 widths, char at idx 7 is space → no split ~keep
    let span = make_decimal_span("Theorem 1.1", vec![9.3, 7.5, 6.1, 6.7, 5.5, 6.1, 11.2], 80.0, 12.0);
    assert!(PdfDocument::char_widths_boundary_split(&span).is_none());
}

#[test]
fn test_cw_boundary_no_split_matching_count() {
    // "hello" with 5 widths: no mismatch ~keep
    let span = make_decimal_span("hello", vec![5.0, 5.0, 5.0, 5.0, 5.0], 25.0, 12.0);
    assert!(PdfDocument::char_widths_boundary_split(&span).is_none());
}

#[test]
fn test_cw_boundary_no_split_nonascii_boundary() {
    // "Marysia Prus-Gł": boundary char is 'ł' (non-ASCII) → no split ~keep
    let span = make_decimal_span("Marysia Prus-Gł", vec![5.0; 14], 80.0, 12.0);
    assert!(PdfDocument::char_widths_boundary_split(&span).is_none());
}

#[test]
fn test_push_span_text_splits_let_capital() {
    // Lower→upper boundary: "LetC" splits to "Let C" (space inserted at 'C') ~keep
    let span = make_decimal_span("LetC", vec![7.3, 5.2, 4.5], 26.7, 12.0);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "Let C");
}

#[test]
fn test_push_span_text_splits_theorem_number() {
    let span = make_decimal_span("Theorem1.7", vec![11.2, 8.9, 7.4, 8.1, 6.6, 7.4, 13.4], 83.8, 14.3);
    let mut out = String::new();
    PdfDocument::push_span_text(&mut out, &span);
    assert_eq!(out, "Theorem 1.7");
}

#[test]
fn test_filter_leaked_metadata_clean_text() {
    let text = "This is normal text without any metadata patterns.";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert_eq!(result, text);
}

#[test]
fn test_filter_leaked_metadata_removes_whitepoint() {
    let text = "Hello World\nWhitePoint [ 0.95 1.0 1.09 ]\nMore text";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.contains("Hello World"));
    assert!(result.contains("More text"));
    assert!(!result.contains("WhitePoint"));
}

#[test]
fn test_filter_leaked_metadata_removes_calrgb() {
    let text = "Text\nCalRGB /WhitePoint [ 1 1 1 ]\nMore";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.contains("Text"));
    assert!(result.contains("More"));
    assert!(!result.contains("CalRGB"));
}

#[test]
fn test_filter_leaked_metadata_preserves_normal_lines() {
    let text = "The Matrix is a movie\nGamma rays from space";
    // These lines contain metadata keywords but not in metadata format ~keep
    let result = PdfDocument::filter_leaked_metadata(text);
    // "The Matrix is a movie" should be preserved (doesn't start with "Matrix") ~keep
    assert!(result.contains("The Matrix is a movie"));
}

#[test]
fn test_normalize_kangxi_no_radicals() {
    let text = "Hello World";
    let result = PdfDocument::normalize_kangxi_radicals(text);
    assert_eq!(result, text);
}

#[test]
fn test_normalize_kangxi_with_radicals() {
    // U+2F00 is Kangxi Radical One ~keep
    let text = "\u{2F00}";
    let result = PdfDocument::normalize_kangxi_radicals(text);
    // Should be normalized to a CJK unified ideograph ~keep
    assert_ne!(result, text);
}

#[test]
fn test_normalize_arabic_no_presentation_forms() {
    let text = "Hello World";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    assert_eq!(result, text);
}

#[test]
fn test_normalize_arabic_alef_presentation_form() {
    // U+FE8D is Arabic Alef isolated form ~keep
    let text = "\u{FE8D}";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    // Should be normalized to base Alef (U+0627) ~keep
    assert!(result.contains('\u{0627}'));
}

#[test]
fn test_normalize_arabic_lam_alef_ligature() {
    // U+FEFB is Lam-Alef ligature ~keep
    let text = "\u{FEFB}";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    // Should become Lam (U+0644) ~keep
    assert!(result.contains('\u{0644}'));
}

// ========================================================================
// reverse_rtl_visual_order_runs tests
// ========================================================================
//
// These tests cover the two distinct RTL span shapes xberg-native-pdf sees
// in the wild and make sure future changes don't regress either:
//
// 1. **Pre-shaped visual-order single span** — one `TextSpan` per
//    line whose `text` already contains contextual Arabic glyphs
//    (U+FB50-U+FDFF / U+FE70-U+FEFF) in the order the content
//    stream drew them (rightmost glyph first). This is the
//    `ArabicCIDTrueType.pdf` pdfjs test fixture case. Expected:
//    character sequence gets reversed in place.
//
// 2. **Plain base-Arabic logical-order single span** — one
//    `TextSpan` per line whose `text` uses base Arabic (U+0621-
//    U+06FF) characters in logical / reading order, as most
//    well-behaved PDF producers emit. Expected: span is left
//    completely alone (no reversal, no shape changes).
//
// The gate that protects case 2 from case 1's reversal is the
// `has_presentation_form` check inside `reverse_rtl_visual_order_runs`. ~keep

fn make_rtl_test_span(text: &str, x: f32, y: f32) -> TextSpan {
    TextSpan {
        text: text.to_string(),
        bbox: crate::geometry::Rect::new(x, y, 100.0, 12.0),
        font_size: 12.0,
        ..TextSpan::default()
    }
}

#[test]
fn test_reverse_rtl_preshaped_single_span() {
    // "ArabicCIDTrueType.pdf" shape: one span per line, glyphs in
    // visual / right-to-left rendering order, mixing presentation
    // form `ﳋ` (U+FCCB) with base Arabic characters. The helper
    // must reverse this into reading order so downstream consumers
    // see logical Arabic even though the content stream is visual. ~keep
    let mut spans = vec![
        make_rtl_test_span(
            "\u{0629}\u{064A}\u{0628}\u{0631}\u{0639}\u{0644}\u{0627} \
                                \u{0637}\u{0648}\u{0637}\u{FCCB}\u{0627} \
                                \u{0639}\u{0627}\u{0648}\u{0646}\u{0627}",
            100.0,
            700.0,
        ),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    // After reversal, the first span should read as
    // "انواع اﳋطوط العربية" — the logical reading order. The
    // exact string comparison is the reversal of the input. ~keep
    assert_eq!(
        spans[0].text,
        "\u{0627}\u{0646}\u{0648}\u{0627}\u{0639} \
             \u{0627}\u{FCCB}\u{0637}\u{0648}\u{0637} \
             \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}",
        "Pre-shaped Arabic single span must be reversed into reading order"
    );
    // Other non-RTL spans must be untouched. ~keep
    assert_eq!(spans[1].text, "other content");
    assert_eq!(spans[2].text, "more content");
    assert_eq!(spans[3].text, "tail");
}

#[test]
fn test_reverse_rtl_logical_order_base_arabic_untouched() {
    // Most Arabic PDFs store text in logical (reading) order using
    // base characters (U+0621-U+06FF) and rely on the renderer to
    // apply shaping at display time. xberg-native-pdf must leave those
    // spans alone — reversing them would garble correct output.
    //
    // The string below is "انواع الخطوط العربية" entirely composed
    // of base Arabic code points (no presentation forms). Gate:
    // `has_presentation_form` stays false, no reversal happens. ~keep
    let logical = "\u{0627}\u{0646}\u{0648}\u{0627}\u{0639} \
                       \u{0627}\u{0644}\u{062E}\u{0637}\u{0648}\u{0637} \
                       \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}";
    let mut spans = vec![
        make_rtl_test_span(logical, 100.0, 700.0),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    assert_eq!(
        spans[0].text, logical,
        "Logical-order base-Arabic span must NOT be reversed"
    );
}

#[test]
fn test_reverse_rtl_short_rtl_span_not_touched_by_pass0() {
    // Pass 0 requires at least 4 non-whitespace characters. A
    // two-character Arabic snippet must not trigger reversal even
    // though it contains presentation forms. ~keep
    let mut spans = vec![
        make_rtl_test_span("\u{FB7F}\u{FEB3}", 100.0, 700.0),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    assert_eq!(spans[0].text, "\u{FB7F}\u{FEB3}");
}

#[test]
fn test_reverse_rtl_pass0_leaves_ltr_alone() {
    // Pure Latin spans never trip the RTL heuristic — `rtl_count`
    // is zero so the majority gate fails. ~keep
    let mut spans = vec![
        make_rtl_test_span("The quick brown fox jumps over", 100.0, 700.0),
        make_rtl_test_span("the lazy dog repeatedly.", 100.0, 680.0),
        make_rtl_test_span("Latin content here.", 100.0, 660.0),
        make_rtl_test_span("Final line.", 100.0, 640.0),
    ];
    let before: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    let after: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    assert_eq!(before, after, "Pure-Latin spans must not be reversed by the RTL pass");
}

// The common CID-TrueType shape — one span PER WORD, each word's
// characters already in LOGICAL order (Presentation Forms), laid out
// right-to-left so the row-aware sort hands them to us left-to-right
// (x ascending: last logical word first). The pass must (A) NOT
// char-reverse the per-word spans — they're already logical — and
// (B) reverse the WORD order so they read right-to-left. Phrase:
// "اﻧﻮاع اﳋﻄﻮط اﻟﻌﺮﺑﻴﺔ" ("types of Arabic fonts"). ~keep
#[test]
fn test_reverse_rtl_per_word_logical_spans_reorder_not_charflip() {
    // Spans in x-ascending order (as emitted by the row-aware sort):
    // العربية (leftmost) … انواع (rightmost / logically first). ~keep
    let mut spans = vec![
        make_rtl_test_span("اﻟﻌﺮﺑﻴﺔ", 160.0, 700.0),
        make_rtl_test_span(" ", 277.0, 700.0),
        make_rtl_test_span("اﳋﻄﻮط", 288.0, 700.0),
        make_rtl_test_span(" ", 409.0, 700.0),
        make_rtl_test_span("اﻧﻮاع", 420.0, 700.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    // (B) word order reversed to logical right-to-left: ~keep
    assert_eq!(
        texts,
        vec!["اﻧﻮاع", " ", "اﳋﻄﻮط", " ", "اﻟﻌﺮﺑﻴﺔ"],
        "per-word RTL spans must be reordered into logical word order \
             without char-flipping (got {texts:?})"
    );
}

// The tagged struct-tree path collapses a page into one MCID
// whose pure-RTL word-spans are laid out left-to-right (visual, X
// ascending). `order_mcid_spans` must emit them right-to-left (logical)
// using geometry, since the tagged path never reaches the untagged
// `reverse_rtl_visual_order_runs`. (Per-span glyph order is handled
// separately by `push_span_text_bidi`; this test asserts span ORDER.) ~keep
#[test]
fn test_order_mcid_spans_pure_rtl_emitted_right_to_left() {
    // One Hebrew row, three words placed left-to-right by X. ~keep
    let spans = vec![
        make_rtl_test_span("שלוש", 100.0, 700.0), // leftmost  → logically last ~keep
        make_rtl_test_span("שתיים", 200.0, 700.0),
        make_rtl_test_span("אחת", 300.0, 700.0), // rightmost → logically first ~keep
    ];
    let ordered = PdfDocument::order_mcid_spans(&spans);
    let texts: Vec<&str> = ordered.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["אחת", "שתיים", "שלוש"],
        "pure-RTL MCID spans must emit rightmost-first (logical RTL order), got {texts:?}"
    );
}

/// SEG-AR cross-span glyph interleave: a word drawn as a body span plus a
/// zero-width consonant span whose x falls INSIDE the body must be repaired
/// (merged + reversed) into correct logical order, not atom-scrambled.
#[test]
fn test_merge_interleaved_rtl_word_reconstructs_logical() {
    use crate::geometry::Rect;
    // الثدييات visual L→R is "ت ا ي ي د ث ل ا"; the producer draws the body
    // "تاييدلا" (7 glyphs @ x=100,110,…,160) and the consonant ث as a
    // zero-width span at x=145, strictly inside the body's [100,170] extent. ~keep
    let body = TextSpan {
        text: "تاييدلا".to_string(),
        bbox: Rect::new(100.0, 700.0, 70.0, 12.0),
        char_widths: vec![10.0; 7],
        char_x_offsets: Vec::new(),
        font_size: 12.0,
        ..TextSpan::default()
    };
    let theh = TextSpan {
        text: "ث".to_string(),
        bbox: Rect::new(145.0, 700.0, 0.0, 12.0),
        font_size: 12.0,
        ..TextSpan::default()
    };
    let spans = [body, theh];
    let line: Vec<&TextSpan> = spans.iter().collect();
    assert!(
        PdfDocument::rtl_line_needs_glyph_reorder(&line),
        "interleaved zero-width consonant must trigger the glyph-reorder gate"
    );
    // The merged span is VISUAL order; push_span_text_bidi reverses it. ~keep
    let merged = PdfDocument::merge_rtl_line_to_visual_span(&line);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &merged, true);
    assert_eq!(out, "الثدييات", "interleaved word not reconstructed, got {out:?}");
}

/// P3: a producer-segmented word boundary between two Arabic words must
/// survive the `merge_rtl_line_to_visual_span` → `push_span_text_bidi`
/// pipeline even when it falls after a DUAL-joining letter (ع in `أنواع`).
/// The merge records the boundary from the producer's STANDALONE space span;
/// without the word-boundary sentinel, `strip_interior_arabic_spaces`'s
/// sparse branch deletes it (a space after a dual-joining letter looks like a
/// cursive-shatter artefact), gluing `أنواع شائعة` → `أنواعشائعة`.
#[test]
fn test_merge_rtl_preserves_producer_word_boundary_after_dual_joining() {
    use crate::geometry::Rect;
    // Two words laid out left-to-right in VISUAL order (logical order is the
    // reverse): word two `شائعة` (visual `ةعئاش`, x 100‒140), a standalone
    // producer space span at x=150, then word one `أنواع` (visual `عاونأ`,
    // x 160‒200). The logical boundary sits after ع (dual-joining). ~keep
    let word_two = TextSpan {
        text: "ةعئاش".to_string(),
        bbox: Rect::new(100.0, 700.0, 50.0, 12.0),
        char_widths: vec![10.0; 5],
        char_x_offsets: Vec::new(),
        font_size: 12.0,
        ..TextSpan::default()
    };
    let space = TextSpan {
        text: " ".to_string(),
        bbox: Rect::new(150.0, 700.0, 8.0, 12.0),
        font_size: 12.0,
        ..TextSpan::default()
    };
    let word_one = TextSpan {
        text: "عاونأ".to_string(),
        bbox: Rect::new(160.0, 700.0, 50.0, 12.0),
        char_widths: vec![10.0; 5],
        char_x_offsets: Vec::new(),
        font_size: 12.0,
        ..TextSpan::default()
    };
    let spans = [word_two, space, word_one];
    let line: Vec<&TextSpan> = spans.iter().collect();
    let merged = PdfDocument::merge_rtl_line_to_visual_span(&line);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &merged, true);
    assert_eq!(
        out, "أنواع شائعة",
        "producer word boundary after a dual-joining letter must be kept, got {out:?}"
    );
    assert!(
        !out.contains(PdfDocument::RTL_WORD_BOUNDARY),
        "word-boundary sentinel must be restored to a space, not leaked: {out:?}"
    );
}

/// P2: a line-final zero-width glyph seated a few points below the baseline
/// (`ي` in `في`, drawn at dy≈3pt, width 0) must stay on its own line's band
/// rather than splitting off and reversing to land after the sentence
/// terminator (`في العالم.` → `ف العالم.ي`). The gated RTL line must collapse
/// to a single merged span with `ي` reattached before the full stop.
#[test]
fn test_merge_keeps_subbaseline_zero_width_glyph_on_line() {
    use crate::geometry::Rect;
    let span = |text: &str, x: f32, y: f32, w: f32| TextSpan {
        text: text.to_string(),
        bbox: Rect::new(x, y, w, 12.0),
        font_size: 13.0,
        ..TextSpan::default()
    };
    // Visual (ascending-x) fragments of "… استهلاكا في العالم.":
    // "." (terminator, leftmost), العالم body, ي (zero-width, dy≈3 low),
    // ف (zero-width), a standalone space, then a body word carrying a
    // zero-width mark INSIDE it so the line trips the glyph-reorder gate. ~keep
    let spans = vec![
        span(".", 95.94, 664.5, 3.48),
        span("ملاعلا", 99.42, 664.5, 27.71),
        span(" ", 127.13, 664.5, 3.38),
        span("ي", 132.92, 661.48, 0.0), // line-final, below baseline, width 0 ~keep
        span("ف", 141.99, 664.99, 0.0),
        span(" ", 145.89, 664.5, 3.38),
        span("االهسا", 149.26, 664.5, 41.64),
        span("كً", 153.57, 666.57, 0.0),
        // ~keep
    ];
    let merged = PdfDocument::merge_interleaved_rtl_lines(&spans).expect("interleaved gate must fire on this RTL line");
    assert_eq!(
        merged.len(),
        1,
        "the sub-baseline ي must stay in the line's band, not split off (got {} spans)",
        merged.len()
    );
    let mut out = String::new();
    for s in &merged {
        PdfDocument::push_span_text_bidi(&mut out, s, true);
    }
    assert!(out.contains("في"), "ي must reattach to ف as the word في; got {out:?}");
    assert!(
        !out.trim_end().ends_with('ي'),
        "ي must not be stranded after the sentence terminator; got {out:?}"
    );
}

/// Negative: a pure-RTL line with NO zero-width interleaved span (logical-
/// order word spans, the BidiSample shape) must NOT trigger the reorder gate,
/// so already-correct RTL pages stay on the unchanged path.
#[test]
fn test_rtl_line_no_interleave_skips_glyph_reorder() {
    let spans = vec![
        make_rtl_test_span("אחת", 300.0, 700.0),
        make_rtl_test_span("שתיים", 200.0, 700.0),
        make_rtl_test_span("שלוש", 100.0, 700.0),
    ];
    let line: Vec<&TextSpan> = spans.iter().collect();
    assert!(
        !PdfDocument::rtl_line_needs_glyph_reorder(&line),
        "no zero-width interleave → gate must stay off (byte-identical path)"
    );
    assert!(
        PdfDocument::merge_interleaved_rtl_lines(&spans).is_none(),
        "no interleaved line → no merge (caller uses original spans)"
    );
}

/// A pure-RTL line whose zero-advance glyphs (hamza seats, marks,
/// producer-positioned consonants) are drawn a couple of points off the
/// baseline must NOT be scattered into separate rows. The fixed quantized
/// row band split them out and emitted them first (the leading stray-alef
/// cluster); font-relative line grouping keeps the whole line together and
/// in rightmost-first order.
#[test]
fn test_order_pure_rtl_spans_keeps_jittery_baseline_in_one_line() {
    // Five Arabic letters on ONE visual line at X = 300..100 (rightmost
    // first is logical), but with ±2pt baseline jitter — two of them drawn
    // above the baseline as a zero-width producer would. Font size 12 →
    // tolerance 6pt, so the 4pt spread stays a single line. ~keep
    let spans = vec![
        make_rtl_test_span("\u{0627}", 300.0, 701.0), // ا  rightmost, +1 ~keep
        make_rtl_test_span("\u{0644}", 250.0, 703.0), // ل  above baseline ~keep
        make_rtl_test_span("\u{0642}", 200.0, 700.0), // ق  baseline ~keep
        make_rtl_test_span("\u{0637}", 150.0, 702.0),
        make_rtl_test_span("\u{0645}", 100.0, 699.0), // م  leftmost, -1 ~keep
    ];
    let ordered = PdfDocument::order_pure_rtl_spans(&spans);
    let texts: Vec<&str> = ordered.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["\u{0627}", "\u{0644}", "\u{0642}", "\u{0637}", "\u{0645}"],
        "jittery-baseline RTL line must stay in one rightmost-first run, got {texts:?}"
    );
}

/// Genuinely separate RTL lines (leading ~1.2x font size) must still break:
/// the font-relative tolerance groups jitter, not whole lines.
#[test]
fn test_order_pure_rtl_spans_breaks_distinct_lines() {
    // Two lines, 14pt apart (font size 12 → tol 6pt, so they split). Each
    // line emits rightmost-first; the top line precedes the bottom line. ~keep
    let spans = vec![
        make_rtl_test_span("\u{0628}", 200.0, 714.0),
        make_rtl_test_span("\u{0627}", 300.0, 714.0),
        make_rtl_test_span("\u{062F}", 200.0, 700.0),
        make_rtl_test_span("\u{062C}", 300.0, 700.0),
    ];
    let ordered = PdfDocument::order_pure_rtl_spans(&spans);
    let texts: Vec<&str> = ordered.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["\u{0627}", "\u{0628}", "\u{062C}", "\u{062F}"],
        "distinct RTL lines must break (top first, each rightmost-first), got {texts:?}"
    );
}

// Grapheme-aware RTL reversal keeps Arabic combining marks bound to
// their base letter (vs. a naive chars().rev() that floats them off). ~keep
#[test]
fn test_reverse_rtl_keeping_marks_keeps_diacritics_attached() {
    // قِطّ = QAF + KASRA(U+0650) + TAH + SHADDA(U+0651). Reversing must
    // keep each mark immediately after its base, not lead the string. ~keep
    let src = "\u{0642}\u{0650}\u{0637}\u{0651}";
    let out = PdfDocument::reverse_rtl_keeping_marks(src);
    // Expected: base order reversed (TAH+SHADDA group, then QAF+KASRA group). ~keep
    assert_eq!(out, "\u{0637}\u{0651}\u{0642}\u{0650}");
    // No combining mark ever leads a base it doesn't belong to: every
    // diacritic is immediately preceded by a non-diacritic. ~keep
    let chars: Vec<char> = out.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if crate::text::rtl_detector::is_rtl_diacritic(*c as u32) {
            assert!(
                i > 0 && !crate::text::rtl_detector::is_rtl_diacritic(chars[i - 1] as u32),
                "diacritic at {i} is detached from its base"
            );
        }
    }
}

/// A neutral-only span ("<space><comma>") inside a pure-RTL run carries its
/// glyphs in visual draw order; emitting it under an RTL run must reverse it
/// to logical order ("<comma><space>") so the comma re-attaches to the word
/// it follows. Reproduces the wiki-cat-he `הטורפים, ממשפחת` case.
#[test]
fn test_push_span_text_bidi_reverses_neutral_span_in_rtl_run() {
    let span = make_rtl_test_span(" ,", 270.0, 700.0);
    let mut out = String::from("\u{05D4}\u{05D8}\u{05D5}\u{05E8}");
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert!(out.ends_with(", "), "neutral span not reversed to logical: {out:?}");
    assert!(!out.ends_with(" ,"), "visual order leaked into output: {out:?}");
}

/// The same neutral-only span in a non-RTL run (rtl_run = false) is emitted
/// verbatim — LTR text keeps visual == logical order, so reversal would be
/// wrong. Pins the no-regression contract for LTR documents.
#[test]
fn test_push_span_text_bidi_keeps_neutral_span_in_ltr_run() {
    let span = make_rtl_test_span(" ,", 270.0, 700.0);
    let mut out = String::from("word");
    PdfDocument::push_span_text_bidi(&mut out, &span, false);
    assert_eq!(out, "word ,", "LTR neutral span must be emitted verbatim");
}

/// A neutral+single-number span inside a pure-RTL run is reversed to logical
/// order with the DIGIT RUN KEPT FORWARD (UAX #9 L2): visual `2009,` →
/// logical `,2009` (the comma re-attaches to the preceding word; `2009` is
/// never flipped to `9002`). Guards `is_reversible_rtl_numeric_span`.
#[test]
fn test_push_span_text_bidi_reverses_neutral_number_keeping_digits() {
    let span = make_rtl_test_span("2009,", 270.0, 700.0);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert_eq!(out, ",2009", "neutral+number span: reverse order, keep 2009 forward");
}

/// A span with TWO digit runs joined by a hyphen (a year range / ORCID) must
/// NOT be reversed — only a single maximal digit run qualifies.
#[test]
fn test_push_span_text_bidi_does_not_reverse_multi_number_span() {
    let span = make_rtl_test_span("2009-2010", 270.0, 700.0);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert_eq!(out, "2009-2010", "multi-digit-run span must be emitted verbatim");
}

#[test]
fn test_is_reversible_rtl_neutral_span_classification() {
    // Reversible: at least one reorderable punctuation mark + ≥2 chars. ~keep
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" ,"));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" ."));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(". "));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" \u{060C}")); // Arabic comma ~keep
    // Not reversible: single char (reverses to itself), bare spaces, or
    // anything carrying a letter / digit / bracket / quote. ~keep
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(","));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span("  "));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" 9"));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" )"));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" \""));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span("a,"));
}

#[test]
fn test_strip_interior_arabic_spaces() {
    // Spurious space between two Arabic letters (cursive join) is dropped.
    // قِ ل ا  →  قِلا  (space between kasra-marked qaf and lam removed). ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642}\u{0650} \u{0644}\u{0627}"),
        "\u{0642}\u{0650}\u{0644}\u{0627}"
    );
    // A combining mark adjacent to the space does not hide the base letter. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642} \u{0650}\u{0644}"),
        "\u{0642}\u{0650}\u{0644}"
    );
    // Leading / trailing spaces (real word-break candidates) are preserved. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces(" \u{0642}\u{0644} "),
        " \u{0642}\u{0644} "
    );
    // Non-Arabic flanks are left alone: Hebrew (non-cursive) keeps its space. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{05E9} \u{05DC}"),
        "\u{05E9} \u{05DC}"
    );
    // Space between an Arabic letter and a digit is a real boundary — kept. ~keep
    assert_eq!(PdfDocument::strip_interior_arabic_spaces("\u{0642} 5"), "\u{0642} 5");
    // No spaces → fast path returns the input unchanged. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642}\u{0644}"),
        "\u{0642}\u{0644}"
    );
    // Joining-type discriminator: a space AFTER a right-joining-only letter
    // (reh ر) is kept — the join already breaks there, so it may be a real
    // word boundary and stripping it would concatenate two words.
    // بحر ما  →  unchanged (reh before the space). ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0628}\u{062D}\u{0631} \u{0645}\u{0627}"),
        "\u{0628}\u{062D}\u{0631} \u{0645}\u{0627}"
    );
    // A space after a DUAL-joining letter (beh ب) unambiguously broke a
    // cursive join → still stripped.  كتب لا  →  كتبلا ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0643}\u{062A}\u{0628} \u{0644}\u{0627}"),
        "\u{0643}\u{062A}\u{0628}\u{0644}\u{0627}"
    );
    // SHATTER: a producer that exploded one word into glyphs (a space between
    // most letter pairs) has every interior space stripped.  ة لي ص ف → ةليصف ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0629} \u{0644}\u{064A} \u{0635} \u{0641}"),
        "\u{0629}\u{0644}\u{064A}\u{0635}\u{0641}"
    );
    // NOT a shatter: sparse spaces in genuine multi-word text are kept (the
    // density stays below half the inter-letter gaps).  دار سلام بلد  unchanged ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces(
            "\u{062F}\u{0627}\u{0631} \u{0633}\u{0644}\u{0627}\u{0645} \u{0628}\u{0644}\u{062F}"
        ),
        "\u{062F}\u{0627}\u{0631} \u{0633}\u{0644}\u{0627}\u{0645} \u{0628}\u{0644}\u{062F}"
    );
}

#[test]
fn test_mcid_run_is_pure_rtl() {
    let pure_rtl = vec![
        make_rtl_test_span("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", 100.0, 700.0),
        make_rtl_test_span(" ,", 90.0, 700.0),
    ];
    assert!(PdfDocument::mcid_run_is_pure_rtl(&pure_rtl));
    // RTL + Latin → not pure-RTL (full UAX #9 deferred). ~keep
    let mixed = vec![
        make_rtl_test_span("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", 100.0, 700.0),
        make_rtl_test_span("World", 200.0, 700.0),
    ];
    assert!(!PdfDocument::mcid_run_is_pure_rtl(&mixed));
    // No RTL at all → not pure-RTL. ~keep
    let ltr = vec![make_rtl_test_span("Hello", 100.0, 700.0)];
    assert!(!PdfDocument::mcid_run_is_pure_rtl(&ltr));
}

// Mixed RTL+Latin MCIDs are left in raw order (full UAX #9 deferred) —
// guards against the pure-RTL reorder accidentally firing on mixed runs. ~keep
#[test]
fn test_order_mcid_spans_mixed_rtl_latin_kept_raw() {
    let spans = vec![
        make_rtl_test_span("שלום", 100.0, 700.0),
        make_rtl_test_span("World", 200.0, 700.0),
    ];
    let ordered = PdfDocument::order_mcid_spans(&spans);
    let texts: Vec<&str> = ordered.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["שלום", "World"], "mixed RTL+Latin must stay in raw order");
}

fn span_wmode(x: f32, y: f32, wmode: u8) -> TextSpan {
    TextSpan {
        text: "x".to_string(),
        bbox: crate::geometry::Rect::new(x, y, 12.0, 12.0),
        wmode,
        ..TextSpan::default()
    }
}

// A page is vertical-writing only when a majority of non-empty spans carry
// WMode 1 — authoritative, so horizontal pages are never misclassified. ~keep
#[test]
fn test_page_is_vertical() {
    let v = [
        span_wmode(0.0, 0.0, 1),
        span_wmode(0.0, 0.0, 1),
        span_wmode(0.0, 0.0, 0),
    ];
    assert!(PdfDocument::page_is_vertical(&v));
    let h = [
        span_wmode(0.0, 0.0, 0),
        span_wmode(0.0, 0.0, 0),
        span_wmode(0.0, 0.0, 1),
    ];
    assert!(!PdfDocument::page_is_vertical(&h));
    // Exact tie is not a majority — stay horizontal (conservative). ~keep
    let tie = [span_wmode(0.0, 0.0, 1), span_wmode(0.0, 0.0, 0)];
    assert!(!PdfDocument::page_is_vertical(&tie));
    assert!(!PdfDocument::page_is_vertical(&[]));
    let mut blank = span_wmode(0.0, 0.0, 1);
    blank.text = "   ".to_string();
    assert!(!PdfDocument::page_is_vertical(std::slice::from_ref(&blank)));
}

// Horizontal pages use the top/bottom band; vertical pages ALSO use the
// left/right band. The side band is additive — it never removes the
// top/bottom membership a horizontal page relies on. ~keep
#[test]
fn test_in_chrome_band() {
    let (w, h) = (612.0_f32, 792.0_f32); // vband=95.04, hband=73.44 ~keep
    let top = crate::geometry::Rect::new(300.0, 780.0, 12.0, 12.0);
    let bottom = crate::geometry::Rect::new(300.0, 10.0, 12.0, 12.0);
    let middle = crate::geometry::Rect::new(300.0, 400.0, 12.0, 12.0);
    let left = crate::geometry::Rect::new(10.0, 400.0, 12.0, 12.0);
    let right = crate::geometry::Rect::new(600.0, 400.0, 12.0, 12.0);

    // Top/bottom are chrome in BOTH modes; middle is never chrome. ~keep
    for vertical in [false, true] {
        assert!(PdfDocument::in_chrome_band(&top, w, h, vertical));
        assert!(PdfDocument::in_chrome_band(&bottom, w, h, vertical));
        assert!(!PdfDocument::in_chrome_band(&middle, w, h, vertical));
    }
    // Side strips: chrome only when vertical. ~keep
    assert!(!PdfDocument::in_chrome_band(&left, w, h, false));
    assert!(!PdfDocument::in_chrome_band(&right, w, h, false));
    assert!(PdfDocument::in_chrome_band(&left, w, h, true));
    assert!(PdfDocument::in_chrome_band(&right, w, h, true));
}

// Bare page-number detection (applied only inside the margin band). ~keep
#[test]
fn test_is_bare_page_number_text() {
    for yes in ["1", "12", "999", "1000", "9999", " 7 ".trim()] {
        assert!(
            PdfDocument::is_bare_page_number_text(yes),
            "{yes:?} should be a page number"
        );
    }
    for no in ["", "0", "10000", "12345", "1a", "iv", "Page", "1.2", "-1", "1,2"] {
        assert!(
            !PdfDocument::is_bare_page_number_text(no),
            "{no:?} must NOT be a page number"
        );
    }
}

// Non-Latin folio digits are recognized as bare page numbers, bounded by
// character count (each is 2-3 UTF-8 bytes) and range-checked via the
// block-offset map (parse/to_digit are ASCII-only). ~keep
#[test]
fn test_is_bare_page_number_text_non_latin() {
    for yes in [
        "\u{0661}",                         // Arabic-Indic ١ = 1 ~keep
        "\u{0661}\u{0662}",                 // ١٢ = 12 ~keep
        "\u{06F3}",                         // Persian ۳ = 3 ~keep
        "\u{06F1}\u{06F2}\u{06F3}",         // ۱۲۳ = 123 ~keep
        "\u{0967}\u{0966}",                 // Devanagari १० = 10 ~keep
        "\u{FF11}\u{FF12}\u{FF13}\u{FF14}", // full-width １２３４ = 1234 ~keep
    ] {
        assert!(
            PdfDocument::is_bare_page_number_text(yes),
            "{yes:?} should be a non-Latin page number"
        );
    }
    for no in [
        "\u{0660}",
        // ~keep
        "\u{FF11}\u{FF10}\u{FF10}\u{FF10}\u{FF10}",
        // ~keep
        "\u{4E00}",  // CJK 一 — ideographic, intentionally excluded ~keep
        "\u{0661}a", // digit + letter ~keep
    ] {
        assert!(
            !PdfDocument::is_bare_page_number_text(no),
            "{no:?} must NOT be a page number"
        );
    }
}

// Folios paginated in non-Latin digits collapse to a shared signature, so
// the varying-literal gate (variants >= 2) can fire. ~keep
#[test]
fn test_normalize_artifact_signature_non_latin_digits() {
    // Persian "صفحه ۱" and "صفحه ۲" must share one signature. ~keep
    let s1 = PdfDocument::normalize_artifact_signature("\u{0635}\u{0641}\u{062D}\u{0647} \u{06F1}");
    let s2 = PdfDocument::normalize_artifact_signature("\u{0635}\u{0641}\u{062D}\u{0647} \u{06F2}");
    assert_eq!(s1, s2, "Persian folios must collapse to one signature");
    assert!(s1.contains('#'), "digit run must collapse to # (got {s1:?})");

    // Full-width "第１頁" / "第２頁" share a signature; multi-digit runs
    // collapse to a single #. ~keep
    let f1 = PdfDocument::normalize_artifact_signature("\u{FF11}\u{FF10}");
    assert_eq!(f1, "#", "full-width digit run collapses to a single #");

    // CJK ideographic numerals are NOT digits: "第一章" must stay intact
    // so real headings are not over-normalized. ~keep
    let heading = PdfDocument::normalize_artifact_signature("\u{7B2C}\u{4E00}\u{7AE0}");
    assert_eq!(
        heading, "\u{7B2C}\u{4E00}\u{7AE0}",
        "ideographic numerals must not collapse"
    );
}

#[test]
fn test_looks_like_stable_pagination() {
    for yes in [
        "https://doi.org/10.1234/abcd",
        "doi:10.1000/xyz",
        "Volume 14 | Article 153",
        "Vol. 7, No. 3",
        "www.frontiersin.org 1",
    ] {
        assert!(
            PdfDocument::looks_like_stable_pagination(yes),
            "{yes:?} should be stable pagination furniture"
        );
    }
    for no in [
        "Acme Regional Hospital",
        "Jane A. Doe",
        "Introduction",
        "Table 3",
        "Department of Neuroscience 2024",
        "volume of distribution", // citation keyword but NO digit ~keep
    ] {
        assert!(
            !PdfDocument::looks_like_stable_pagination(no),
            "{no:?} must NOT be classified as furniture"
        );
    }
}

#[test]
fn test_decode_pdf_text_string_utf16be() {
    let bytes = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    assert_eq!(result, "AB");
}

#[test]
fn test_decode_pdf_text_string_utf16le() {
    let bytes = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    assert_eq!(result, "AB");
}

#[test]
fn test_decode_pdf_text_string_pdfdoc_encoding() {
    let bytes = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    assert_eq!(result, "Hello");
}

#[test]
fn test_decode_pdf_text_string_empty() {
    let bytes: Vec<u8> = vec![];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    assert_eq!(result, "");
}

#[test]
fn test_strip_xhtml_tags_basic() {
    let xhtml = "<p>Hello <b>World</b></p>";
    let result = PdfDocument::strip_xhtml_tags(xhtml);
    assert_eq!(result, "Hello World");
}

#[test]
fn test_strip_xhtml_tags_no_tags() {
    let text = "Plain text without any tags";
    let result = PdfDocument::strip_xhtml_tags(text);
    assert_eq!(result, text);
}

#[test]
fn test_strip_xhtml_tags_empty() {
    assert_eq!(PdfDocument::strip_xhtml_tags(""), "");
}

#[test]
fn test_strip_xhtml_tags_nested() {
    let xhtml = "<div><p><span style='color: red'>Red text</span></p></div>";
    let result = PdfDocument::strip_xhtml_tags(xhtml);
    assert_eq!(result, "Red text");
}

#[test]
fn test_parse_string_value_static_string() {
    let obj = Object::String(b"Hello".to_vec());
    let result = PdfDocument::parse_string_value_static(Some(&obj));
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "Hello");
}

#[test]
fn test_parse_string_value_static_name() {
    let obj = Object::Name("MyName".to_string());
    let result = PdfDocument::parse_string_value_static(Some(&obj));
    assert_eq!(result, Some("MyName".to_string()));
}

#[test]
fn test_parse_string_value_static_integer() {
    let obj = Object::Integer(42);
    let result = PdfDocument::parse_string_value_static(Some(&obj));
    assert_eq!(result, Some("42".to_string()));
}

#[test]
fn test_parse_string_value_static_real() {
    let obj = Object::Real(std::f64::consts::PI);
    let result = PdfDocument::parse_string_value_static(Some(&obj));
    assert!(result.is_some());
    let s = result.unwrap();
    assert!(s.starts_with("3.14"));
}

#[test]
fn test_parse_string_value_static_null() {
    let obj = Object::Null;
    let result = PdfDocument::parse_string_value_static(Some(&obj));
    assert!(result.is_none());
}

#[test]
fn test_parse_string_value_static_none() {
    let result = PdfDocument::parse_string_value_static(None);
    assert!(result.is_none());
}

#[test]
fn test_find_references_reference() {
    let obj = Object::Reference(ObjectRef::new(5, 0));
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0], ObjectRef::new(5, 0));
}

#[test]
fn test_find_references_array() {
    let arr = Object::Array(vec![
        Object::Reference(ObjectRef::new(1, 0)),
        Object::Integer(42),
        Object::Reference(ObjectRef::new(2, 0)),
    ]);
    let refs = PdfDocument::find_references(&arr);
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_find_references_dictionary() {
    let mut dict = std::collections::HashMap::new();
    dict.insert("Key1".to_string(), Object::Reference(ObjectRef::new(3, 0)));
    dict.insert("Key2".to_string(), Object::Integer(1));
    let obj = Object::Dictionary(dict);
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_find_references_stream() {
    let mut dict = std::collections::HashMap::new();
    dict.insert("Length".to_string(), Object::Reference(ObjectRef::new(10, 0)));
    let obj = Object::Stream {
        dict,
        data: bytes::Bytes::from_static(b""),
    };
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_find_references_integer() {
    let refs = PdfDocument::find_references(&Object::Integer(42));
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_null() {
    let refs = PdfDocument::find_references(&Object::Null);
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_boolean() {
    let refs = PdfDocument::find_references(&Object::Boolean(true));
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_nested() {
    let inner = Object::Array(vec![Object::Reference(ObjectRef::new(7, 0))]);
    let mut dict = std::collections::HashMap::new();
    dict.insert("Inner".to_string(), inner);
    dict.insert("Direct".to_string(), Object::Reference(ObjectRef::new(8, 0)));
    let obj = Object::Dictionary(dict);
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_find_substring_found() {
    assert_eq!(find_substring(b"Hello World", b"World"), Some(6));
}

#[test]
fn test_find_substring_not_found() {
    assert_eq!(find_substring(b"Hello World", b"xyz"), None);
}

#[test]
fn test_find_substring_empty_needle() {
    assert_eq!(find_substring(b"Hello", b""), Some(0));
}

#[test]
fn test_find_substring_at_start() {
    assert_eq!(find_substring(b"Hello", b"Hello"), Some(0));
}

#[test]
fn test_find_substring_at_end() {
    assert_eq!(find_substring(b"Hello", b"lo"), Some(3));
}

#[test]
fn test_find_substring_empty_haystack() {
    assert_eq!(find_substring(b"", b"Hello"), None);
}

#[test]
fn test_parse_matrix_from_object_valid() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Real(1.0),
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(1.0),
        Object::Real(10.0),
        Object::Real(20.0),
    ]);
    let matrix = doc.parse_matrix_from_object(&arr).unwrap();
    assert!((matrix.a - 1.0).abs() < f32::EPSILON);
    assert!((matrix.e - 10.0).abs() < f32::EPSILON);
    assert!((matrix.f - 20.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_matrix_from_object_integers() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Integer(2),
        Object::Integer(0),
        Object::Integer(0),
        Object::Integer(3),
        Object::Integer(100),
        Object::Integer(200),
    ]);
    let matrix = doc.parse_matrix_from_object(&arr).unwrap();
    assert!((matrix.a - 2.0).abs() < f32::EPSILON);
    assert!((matrix.d - 3.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_matrix_from_object_too_short() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![Object::Real(1.0), Object::Real(0.0)]);
    let result = doc.parse_matrix_from_object(&arr);
    assert!(result.is_none());
}

#[test]
fn test_parse_matrix_from_object_not_array() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let result = doc.parse_matrix_from_object(&Object::Integer(42));
    assert!(result.is_none());
}

#[test]
fn test_parse_matrix_from_object_invalid_elements() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Real(1.0),
        Object::Name("bad".to_string()),
        Object::Real(0.0),
        Object::Real(1.0),
        Object::Real(0.0),
        Object::Real(0.0),
    ]);
    let result = doc.parse_matrix_from_object(&arr);
    assert!(result.is_none());
}

#[test]
fn test_transform_bbox_identity() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix::identity();
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.x - 10.0).abs() < f32::EPSILON);
    assert!((result.y - 20.0).abs() < f32::EPSILON);
    assert!((result.width - 100.0).abs() < f32::EPSILON);
    assert!((result.height - 50.0).abs() < f32::EPSILON);
}

#[test]
fn test_transform_bbox_translation() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 50.0,
        f: 100.0,
    };
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.x - 50.0).abs() < f32::EPSILON);
    assert!((result.y - 100.0).abs() < f32::EPSILON);
}

#[test]
fn test_transform_bbox_scaling() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 3.0,
        e: 0.0,
        f: 0.0,
    };
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.width - 200.0).abs() < f32::EPSILON);
    assert!((result.height - 150.0).abs() < f32::EPSILON);
}

#[test]
fn test_font_identity_hash_same_font() {
    let mut dict1 = std::collections::HashMap::new();
    dict1.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
    dict1.insert("Subtype".to_string(), Object::Name("Type1".to_string()));

    let mut dict2 = std::collections::HashMap::new();
    dict2.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
    dict2.insert("Subtype".to_string(), Object::Name("Type1".to_string()));

    let hash1 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(dict1));
    let hash2 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(dict2));
    assert_eq!(hash1, hash2);
}

#[test]
fn test_font_identity_hash_different_fonts() {
    let mut dict1 = std::collections::HashMap::new();
    dict1.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));

    let mut dict2 = std::collections::HashMap::new();
    dict2.insert("BaseFont".to_string(), Object::Name("Times-Roman".to_string()));

    let hash1 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(dict1));
    let hash2 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(dict2));
    assert_ne!(hash1, hash2);
}

#[test]
fn test_font_identity_hash_null_object() {
    let hash = PdfDocument::font_identity_hash_cheap(&Object::Null);
    // Should not panic, returns some hash ~keep
    let _ = hash;
}

// Two non-subset fonts sharing BaseFont/Subtype/Encoding but with
// different /Widths must NOT share a cross-document cache key. ~keep
#[test]
fn test_font_identity_hash_differs_on_widths() {
    let base = || {
        let mut d = std::collections::HashMap::new();
        d.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
        d.insert("Subtype".to_string(), Object::Name("Type1".to_string()));
        d.insert("FirstChar".to_string(), Object::Integer(65));
        d.insert("LastChar".to_string(), Object::Integer(67));
        d
    };
    let mut a = base();
    a.insert(
        "Widths".to_string(),
        Object::Array(vec![Object::Integer(600), Object::Integer(600), Object::Integer(600)]),
    );
    let mut b = base();
    b.insert(
        "Widths".to_string(),
        Object::Array(vec![Object::Integer(667), Object::Integer(667), Object::Integer(722)]),
    );

    let hash_a = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(a));
    let hash_b = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(b));
    assert_ne!(
        hash_a, hash_b,
        "fonts with identical BaseFont but different /Widths must not collide"
    );

    let mut c = base();
    c.insert(
        "Widths".to_string(),
        Object::Array(vec![Object::Integer(600), Object::Integer(600), Object::Integer(600)]),
    );
    let mut a2 = base();
    a2.insert(
        "Widths".to_string(),
        Object::Array(vec![Object::Integer(600), Object::Integer(600), Object::Integer(600)]),
    );
    assert_eq!(
        PdfDocument::font_identity_hash_cheap(&Object::Dictionary(c)),
        PdfDocument::font_identity_hash_cheap(&Object::Dictionary(a2)),
        "identical fonts must still share a cache key"
    );
}

// Vertical metrics live on the descendant CIDFont. Their resolved content,
// never the PDF-local object number, determines shared font identity. ~keep
#[test]
fn vertical_metrics_differentiate_font_cache_key() {
    let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let same_w2 = Object::Array(vec![
        Object::Integer(1),
        Object::Integer(3),
        Object::Array(vec![Object::Integer(880), Object::Integer(-500), Object::Integer(500)]),
    ]);
    let different_w2 = Object::Array(vec![
        Object::Integer(1),
        Object::Integer(3),
        Object::Array(vec![Object::Integer(880), Object::Integer(-600), Object::Integer(600)]),
    ]);
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(100, 0), same_w2.clone());
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(200, 0), same_w2);
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(201, 0), different_w2);
    let font = |w2_reference, default_vertical_advance| {
        let descendant = Object::Dictionary(std::collections::HashMap::from([
            ("Subtype".to_string(), Object::Name("CIDFontType2".to_string())),
            (
                "DW2".to_string(),
                Object::Array(vec![Object::Integer(880), Object::Integer(default_vertical_advance)]),
            ),
            ("W2".to_string(), Object::Reference(w2_reference)),
        ]));
        Object::Dictionary(std::collections::HashMap::from([
            ("BaseFont".to_string(), Object::Name("Identity-CIDFont".to_string())),
            ("Subtype".to_string(), Object::Name("Type0".to_string())),
            ("DescendantFonts".to_string(), Object::Array(vec![descendant])),
        ]))
    };

    let hash_100 = doc.font_identity_hash_with_descendants(&font(ObjectRef::new(100, 0), -1000));
    let hash_200 = doc.font_identity_hash_with_descendants(&font(ObjectRef::new(200, 0), -1000));
    let different_w2_hash = doc.font_identity_hash_with_descendants(&font(ObjectRef::new(201, 0), -1000));
    let different_dw2_hash = doc.font_identity_hash_with_descendants(&font(ObjectRef::new(100, 0), -880));

    assert_eq!(
        hash_100, hash_200,
        "identical indirect /W2 content at different object numbers must share a cache key"
    );
    assert_ne!(
        hash_100, different_w2_hash,
        "different descendant /W2 content must not share a cache key"
    );
    assert_ne!(
        hash_100, different_dw2_hash,
        "different descendant /DW2 content must not share a cache key"
    );
}

// Type 3 fonts are document-local and must be kept out of the
// cross-document global font cache (Layer 6). The gate uses
// font_is_document_local; pin its classification here. ~keep
#[test]
fn test_type3_font_is_document_local() {
    let mut type3 = std::collections::HashMap::new();
    type3.insert("Subtype".to_string(), Object::Name("Type3".to_string()));
    type3.insert("Name".to_string(), Object::Name("F1".to_string()));
    assert!(
        PdfDocument::font_is_document_local(&Object::Dictionary(type3)),
        "Type3 fonts must be treated as document-local (uncacheable cross-document)"
    );

    for subtype in ["Type1", "TrueType", "Type0", "CIDFontType2"] {
        let mut d = std::collections::HashMap::new();
        d.insert("Subtype".to_string(), Object::Name(subtype.to_string()));
        d.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
        assert!(
            !PdfDocument::font_is_document_local(&Object::Dictionary(d)),
            "{subtype} must remain cacheable across documents"
        );
    }
    assert!(!PdfDocument::font_is_document_local(&Object::Null));

    // Subset fonts (six uppercase letters + '+', ISO 32000-1 §9.6.4) are
    // document-local regardless of subtype — their glyph subset and
    // ToUnicode are document-specific and must not be shared cross-document. ~keep
    for subtype in ["Type1", "TrueType", "Type0", "CIDFontType2"] {
        let mut d = std::collections::HashMap::new();
        d.insert("Subtype".to_string(), Object::Name(subtype.to_string()));
        d.insert(
            "BaseFont".to_string(),
            Object::Name("AAAAAA+ArialUnicodeMS".to_string()),
        );
        assert!(
            PdfDocument::font_is_document_local(&Object::Dictionary(d)),
            "subset {subtype} must be treated as document-local"
        );
    }

    // Subset-prefix edge cases: a 6-uppercase name without '+', a lowercase
    // tag, a short tag, and an empty real name are NOT subsets — stay cacheable. ~keep
    for name in ["ARIALX", "abcdef+Real", "AAAAA+Short", "AAAAAA+"] {
        let mut d = std::collections::HashMap::new();
        d.insert("Subtype".to_string(), Object::Name("Type0".to_string()));
        d.insert("BaseFont".to_string(), Object::Name(name.to_string()));
        assert!(
            !PdfDocument::font_is_document_local(&Object::Dictionary(d)),
            "{name} is not a subset tag and must remain cacheable"
        );
    }

    // A non-Type3 font missing /BaseFont fails safe to document-local. ~keep
    let mut no_basefont = std::collections::HashMap::new();
    no_basefont.insert("Subtype".to_string(), Object::Name("Type0".to_string()));
    assert!(
        PdfDocument::font_is_document_local(&Object::Dictionary(no_basefont)),
        "a non-Type3 font with no /BaseFont must fail safe to document-local"
    );
}

#[test]
fn test_check_for_circular_references_runs() {
    // Minimal PDFs naturally have Page <-> Pages parent references,
    // so we just verify the function runs without panicking
    // returns a list (which may include the Page<->Pages backreference). ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let cycles = doc.check_for_circular_references();
    let _ = cycles;
}

#[test]
fn test_is_form_xobject_nonexistent_ref() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    // Non-existent object should return true (conservative) ~keep
    let result = doc.is_form_xobject(ObjectRef::new(999, 0));
    assert!(result);
}

#[test]
fn test_is_form_xobject_catalog_not_form() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let _ = doc.load_object(ObjectRef::new(1, 0));
    let result = doc.is_form_xobject(ObjectRef::new(1, 0));
    assert!(!result);
}

#[test]
fn test_from_bytes_with_v2_header() {
    let mut pdf = b"%PDF-2.0\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.version(), (2, 0));
}

#[test]
fn test_parse_version_from_header_strict_valid() {
    let header = *b"%PDF-1.7";
    let (major, minor) = parse_version_from_header(&header, false).unwrap();
    assert_eq!((major, minor), (1, 7));
}

#[test]
fn test_parse_version_from_header_strict_invalid_dot() {
    let header = *b"%PDF-1X7";
    let result = parse_version_from_header(&header, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_version_from_header_lenient_invalid_dot() {
    let header = *b"%PDF-1X7";
    let (major, minor) = parse_version_from_header(&header, true).unwrap();
    assert_eq!((major, minor), (1, 4));
}

#[test]
fn lenient_version_warning_has_stable_identity() {
    let header = *b"%PDF-1X7";
    let (result, events) = capture_events(|| parse_version_from_header(&header, true));

    assert_eq!(result.unwrap(), (1, 4));
    let warnings: Vec<_> = events.iter().filter(|event| event.level == Level::WARN).collect();
    assert_eq!(warnings.len(), 1, "expected exactly one recovery warning: {events:#?}");
    assert_eq!(warnings[0].target, format!("{}::document", crate::LOG_TARGET_ROOT));
    assert_eq!(
        warnings[0].fields.get("operation").map(String::as_str),
        Some("parse_pdf_version")
    );
    assert_eq!(
        warnings[0].fields.get("reason").map(String::as_str),
        Some("invalid_version_separator")
    );
}

#[test]
fn test_parse_version_from_header_strict_non_digit() {
    let header = *b"%PDF-X.Y";
    let result = parse_version_from_header(&header, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_version_from_header_lenient_non_digit() {
    let header = *b"%PDF-X.Y";
    let (major, minor) = parse_version_from_header(&header, true).unwrap();
    assert_eq!((major, minor), (1, 4));
}

#[test]
fn test_parse_version_from_header_strict_too_high() {
    let header = *b"%PDF-3.0";
    let result = parse_version_from_header(&header, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_version_from_header_lenient_too_high() {
    let header = *b"%PDF-3.0";
    let (major, minor) = parse_version_from_header(&header, true).unwrap();
    assert_eq!((major, minor), (1, 4));
}

#[test]
fn test_parse_version_from_header_wrong_magic() {
    let header = *b"NotPDF17";
    let result = parse_version_from_header(&header, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_header_empty_file_strict() {
    let mut cursor = Cursor::new(b"");
    let result = parse_header(&mut cursor, false);
    assert!(result.is_err());
}

#[test]
fn test_parse_header_empty_file_lenient() {
    let mut cursor = Cursor::new(b"");
    let result = parse_header(&mut cursor, true);
    assert!(result.is_err());
}

#[test]
fn test_parse_header_very_short_lenient() {
    let mut cursor = Cursor::new(b"AB");
    let result = parse_header(&mut cursor, true);
    let (major, minor, _) = result.unwrap();
    assert_eq!((major, minor), (1, 4));
}

#[test]
fn test_parse_header_header_near_end_of_buffer() {
    // Header at position 8100 (within 8192 byte search window) ~keep
    let mut data = vec![0u8; 8100];
    data.extend_from_slice(b"%PDF-1.6");
    data.extend_from_slice(b"\nrest of file data here");
    let mut cursor = Cursor::new(data);
    let (major, minor, offset) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor, offset), (1, 6, 8100));
}

#[test]
fn test_parse_trailer_with_extra_data() {
    let data = b"some xref data\ntrailer\n<< /Size 10 /Root 1 0 R /Info 2 0 R >>\nstartxref\n100\n";
    let mut cursor = Cursor::new(data);
    let trailer = parse_trailer(&mut cursor).unwrap();
    let dict = trailer.as_dict().unwrap();
    assert_eq!(dict.get("Size").unwrap().as_integer(), Some(10));
}

#[test]
fn test_parse_trailer_empty_after_keyword() {
    let data = b"trailer";
    let mut cursor = Cursor::new(data);
    let result = parse_trailer(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_decode_stream_with_encryption_null_object() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc
        .decode_stream_with_encryption(&Object::Null, ObjectRef::new(1, 0))
        .unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_page_cannot_have_text_no_resources() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let page_dict = std::collections::HashMap::new();
    assert!(doc.page_cannot_have_text(&page_dict));
}

#[test]
fn test_page_cannot_have_text_with_font_resources() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let mut font_dict = std::collections::HashMap::new();
    font_dict.insert("F1".to_string(), Object::Reference(ObjectRef::new(10, 0)));

    let mut resources_dict = std::collections::HashMap::new();
    resources_dict.insert("Font".to_string(), Object::Dictionary(font_dict));

    let mut page_dict = std::collections::HashMap::new();
    page_dict.insert("Resources".to_string(), Object::Dictionary(resources_dict));

    assert!(!doc.page_cannot_have_text(&page_dict));
}

#[test]
fn test_page_cannot_have_text_empty_font_dict() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let font_dict = std::collections::HashMap::new();

    let mut resources_dict = std::collections::HashMap::new();
    resources_dict.insert("Font".to_string(), Object::Dictionary(font_dict));

    let mut page_dict = std::collections::HashMap::new();
    page_dict.insert("Resources".to_string(), Object::Dictionary(resources_dict));

    assert!(doc.page_cannot_have_text(&page_dict));
}

#[test]
fn test_extract_images_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());
}

#[test]
fn test_extract_images_graphics_only() {
    let content = b"100 200 300 400 re S";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());
}

#[test]
fn test_extract_paths_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let paths = doc.extract_paths(0).unwrap();
    assert!(paths.is_empty());
}

#[test]
fn test_extract_paths_rectangle() {
    let content = b"100 200 300 400 re S";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let paths = doc.extract_paths(0).unwrap();
    assert!(!paths.is_empty());
}

#[test]
fn test_mark_info_untagged_pdf() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mark_info = doc.mark_info().unwrap();
    assert!(!mark_info.marked);
    assert!(!mark_info.suspects);
}

#[test]
fn test_extracted_image_ref_debug() {
    let img_ref = ExtractedImageRef {
        filename: "img_001.png".to_string(),
        format: ImageFormat::Png,
        width: 100,
        height: 200,
        bbox: None,
        rotation: 0,
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    let debug = format!("{:?}", img_ref);
    assert!(debug.contains("img_001.png"));
    assert!(debug.contains("Png"));
}

#[test]
fn test_extracted_image_ref_clone() {
    let img_ref = ExtractedImageRef {
        filename: "img_001.jpg".to_string(),
        format: ImageFormat::Jpeg,
        width: 100,
        height: 200,
        bbox: None,
        rotation: 0,
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    let cloned = img_ref.clone();
    assert_eq!(img_ref, cloned);
}

#[test]
fn test_image_format_equality() {
    assert_eq!(ImageFormat::Png, ImageFormat::Png);
    assert_eq!(ImageFormat::Jpeg, ImageFormat::Jpeg);
    assert_ne!(ImageFormat::Png, ImageFormat::Jpeg);
}

#[test]
fn test_apply_intelligent_text_processing_empty() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let spans: Vec<TextSpan> = vec![];
    let result = doc.apply_intelligent_text_processing(spans);
    assert!(result.is_empty());
}

#[test]
fn test_apply_intelligent_text_processing_ligature_preserved() {
    // The pipeline preserves Unicode ligature characters that come
    // from the font's ToUnicode map (U+FB01 = ﬁ). Expanding them to plain "fi"
    // caused Jaccard mismatches against ground-truth corpora that keep ligatures. ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let spans = vec![make_test_span("\u{FB01}nd", 0.0, 0.0, 50.0, 12.0)];
    let result = doc.apply_intelligent_text_processing(spans);
    assert_eq!(result.len(), 1);
    assert!(
        result[0].text.contains('\u{FB01}'),
        "ﬁ must be preserved, got: {:?}",
        result[0].text
    );
}

#[test]
fn test_get_page_caching() {
    let pdf = build_multi_page_pdf(3);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let _page1 = doc.get_page(0).unwrap();
    let _page2 = doc.get_page(0).unwrap();
}

#[test]
fn test_get_page_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc.get_page(99);
    assert!(result.is_err());
}

#[test]
#[allow(deprecated)]
fn test_page_count_u32_returns_correct_value() {
    let pdf = build_multi_page_pdf(3);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count_u32(), 3);
}

#[test]
fn test_structure_tree_untagged_pdf() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let tree = doc.structure_tree().unwrap();
    assert!(tree.is_none());
}

#[test]
fn test_open_with_config() {
    let pdf = build_minimal_pdf(b"");
    let dir = tempfile::tempdir().expect("create temp dir");
    let tmp_path = dir.path().join("native_pdf_test_open_with_config.pdf");
    std::fs::write(&tmp_path, &pdf).unwrap();
    let config = 42u32;
    let result = PdfDocument::open_with_config(&tmp_path, config);
    let _ = std::fs::remove_file(&tmp_path);
    assert!(result.is_ok());
}

#[test]
fn test_get_page_for_debug() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page = doc.get_page_for_debug(0).unwrap();
    assert!(page.as_dict().is_some());
}

#[test]
fn test_may_contain_text_public() {
    assert!(PdfDocument::may_contain_text_public(b"BT /F1 12 Tf ET"));
    assert!(!PdfDocument::may_contain_text_public(b"100 200 re S"));
}

#[test]
fn test_page_inherits_mediabox() {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 400 600] >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 1);
    let page = doc.get_page(0).unwrap();
    let page_dict = page.as_dict().unwrap();
    assert!(page_dict.contains_key("MediaBox"));
}

#[test]
fn test_page_with_array_contents() {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents [4 0 R 5 0 R] /Resources << >> >>\nendobj\n",
        );

    let content1 = b"q";
    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content1.len()).as_bytes());
    pdf.extend_from_slice(content1);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let content2 = b"Q";
    let off5 = pdf.len();
    pdf.extend_from_slice(format!("5 0 obj\n<< /Length {} >>\nstream\n", content2.len()).as_bytes());
    pdf.extend_from_slice(content2);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let data = doc.get_page_content_data(0).unwrap();
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("q"));
    assert!(text.contains("Q"));
}

#[test]
fn test_extract_hierarchical_content_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc.extract_hierarchical_content(0);
    // Should not crash, may return Ok(Some) or Ok(None) ~keep
    assert!(result.is_ok());
}

#[test]
fn test_extract_paths_in_rect_empty_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let region = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 612.0,
        height: 792.0,
    };
    let paths = doc.extract_paths_in_rect(0, region).unwrap();
    assert!(paths.is_empty());
}

#[test]
fn test_nested_page_tree() {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 2 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 /Parent 2 0 R >>\nendobj\n");

    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792] >>\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792] >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 2);
}

#[test]
fn test_mark_info_tagged_pdf() {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true /Suspects false >> >>\nendobj\n",
    );

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mark_info = doc.mark_info().unwrap();
    assert!(mark_info.marked);
    assert!(!mark_info.suspects);
}

#[test]
fn test_extract_spans_with_config_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let config = crate::extractors::SpanMergingConfig::default();
    let spans = doc.extract_spans_with_config(0, config).unwrap();
    assert!(spans.is_empty());
}

#[test]
fn test_get_page_ref_valid() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_ref = doc.get_page_ref(0).unwrap();
    // Page should be object 3 (catalog=1, pages=2, page=3) ~keep
    assert_eq!(page_ref.id, 3);
}

#[test]
fn test_get_page_ref_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc.get_page_ref(99);
    assert!(result.is_err());
}

#[test]
fn test_decode_pdf_text_string_utf8_bom_treated_as_pdfdoc() {
    // UTF-8 BOM (EF BB BF) is NOT recognized by this function;
    // it only handles UTF-16 BOMs. Bytes fall through to PDFDocEncoding. ~keep
    let bytes = vec![0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    // 0xEF -> ï, 0xBB -> », 0xBF -> ¿ in PDFDocEncoding (Latin-1 range) ~keep
    assert_eq!(result, "\u{00EF}\u{00BB}\u{00BF}Hello");
}

#[test]
fn test_decode_pdf_text_string_plain_ascii() {
    let result = PdfDocument::decode_pdf_text_string(b"Hello World");
    assert_eq!(result, "Hello World");
}

#[test]
fn test_decode_pdf_text_string_with_special_chars() {
    let bytes = vec![128u8];
    let result = PdfDocument::decode_pdf_text_string(&bytes);
    assert!(result.contains('\u{2022}'));
}

#[test]
fn test_filter_leaked_metadata_blackpoint() {
    let text = "BlackPoint [ 0 0 0 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_gamma() {
    let text = "Some text\nGamma [ 2.2 2.2 2.2 ]\nMore text";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(!result.contains("Gamma"));
    assert!(result.contains("Some text"));
    assert!(result.contains("More text"));
}

#[test]
fn test_filter_leaked_metadata_matrix_start_line() {
    let text = "Matrix [ 1 0 0 1 0 0 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_calgray() {
    let text = "CalGray /WhitePoint [ 1 1 1 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(!result.contains("CalGray"));
}

#[test]
fn test_filter_leaked_metadata_whitepoint_with_slash() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint /something");
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_whitepoint_with_angle() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint << /Key /Value >>");
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_empty_metadata_value() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint");
    assert!(result.trim().is_empty());
}

#[test]
fn test_normalize_arabic_hamza() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE80}");
    assert!(result.contains('\u{0621}'));
}

#[test]
fn test_normalize_arabic_beh() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE8F}");
    assert!(result.contains('\u{0628}'));
}

#[test]
fn test_normalize_arabic_teh_marbuta() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE93}");
    assert!(result.contains('\u{0629}'));
}

#[test]
fn test_normalize_arabic_dal_to_yeh_range() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEA9}").contains('\u{062F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAB}").contains('\u{0630}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAD}").contains('\u{0631}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAF}").contains('\u{0632}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB1}").contains('\u{0633}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB5}").contains('\u{0634}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB9}").contains('\u{0635}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEBD}").contains('\u{0636}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC1}").contains('\u{0637}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC5}").contains('\u{0638}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC9}").contains('\u{0639}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FECD}").contains('\u{063A}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED1}").contains('\u{0641}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED5}").contains('\u{0642}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED9}").contains('\u{0643}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEDD}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE1}").contains('\u{0645}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE5}").contains('\u{0646}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE9}").contains('\u{0647}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEED}").contains('\u{0648}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEEF}").contains('\u{0649}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF1}").contains('\u{064A}'));
}

#[test]
fn test_normalize_arabic_diacritics() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE70}").contains('\u{064B}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE71}").contains('\u{064B}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE72}").contains('\u{064C}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE74}").contains('\u{064D}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE76}").contains('\u{064E}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE77}").contains('\u{064E}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE78}").contains('\u{064F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE79}").contains('\u{064F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7A}").contains('\u{0650}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7B}").contains('\u{0650}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7C}").contains('\u{0651}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7D}").contains('\u{0651}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7E}").contains('\u{0652}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7F}").contains('\u{0652}'));
}

#[test]
fn test_normalize_arabic_lam_alef_ligatures() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF5}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF7}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF9}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEFB}").contains('\u{0644}'));
}

#[test]
fn test_normalize_arabic_alef_variants() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE81}").contains('\u{0622}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE83}").contains('\u{0623}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE85}").contains('\u{0624}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE87}").contains('\u{0625}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE89}").contains('\u{0626}'));
}

#[test]
fn test_normalize_arabic_mixed_text() {
    let result = PdfDocument::normalize_arabic_presentation_forms("Hello \u{FE8D} World");
    assert!(result.contains("Hello"));
    assert!(result.contains("World"));
    assert!(result.contains('\u{0627}'));
}

#[test]
fn test_strip_xhtml_tags_self_closing() {
    assert_eq!(PdfDocument::strip_xhtml_tags("Hello<br/>World"), "HelloWorld");
}

#[test]
fn test_strip_xhtml_tags_with_attributes() {
    assert_eq!(
        PdfDocument::strip_xhtml_tags("<p class=\"body\">Content</p>"),
        "Content"
    );
}

#[test]
fn test_strip_xhtml_tags_multiple() {
    assert_eq!(
        PdfDocument::strip_xhtml_tags("<b>Bold</b> and <i>Italic</i>"),
        "Bold and Italic"
    );
}

#[test]
fn test_should_insert_space_overlapping() {
    let prev = make_test_span("Hello", 0.0, 100.0, 50.0, 12.0);
    let current = make_test_span("World", 40.0, 100.0, 50.0, 12.0);
    assert!(!PdfDocument::should_insert_space(&prev, &current));
}

#[test]
fn test_should_insert_space_zero_font_size() {
    let prev = make_test_span("A", 0.0, 100.0, 10.0, 0.0);
    let current = make_test_span("B", 15.0, 100.0, 10.0, 0.0);
    let _ = PdfDocument::should_insert_space(&prev, &current);
}

#[test]
fn test_should_insert_space_large_font() {
    let prev = make_test_span("A", 0.0, 100.0, 100.0, 72.0);
    let current = make_test_span("B", 120.0, 100.0, 100.0, 72.0);
    assert!(PdfDocument::should_insert_space(&prev, &current));
}

// SEG-KO: a Sino-Korean numeral hugs its counter ("1만년"), so a tightly
// typeset Hangul↔digit boundary must NOT get a forced space. ~keep
#[test]
fn test_should_insert_space_hangul_digit_no_space() {
    let one = make_test_span("1", 0.0, 100.0, 8.0, 12.0);
    let man = make_test_span("만", 8.5, 100.0, 12.0, 12.0);
    assert!(!PdfDocument::should_insert_space(&one, &man));
    let nyeon = make_test_span("년", 0.0, 100.0, 12.0, 12.0);
    let two = make_test_span("2", 12.5, 100.0, 8.0, 12.0);
    assert!(!PdfDocument::should_insert_space(&nyeon, &two));
}

// The Hangul exception must NOT relax the Chinese ideograph↔digit split
// ("神鹰集团" + "2015" → separate tokens, issue 484). ~keep
#[test]
fn test_should_insert_space_ideograph_digit_still_splits() {
    let tuan = make_test_span("团", 0.0, 100.0, 12.0, 12.0);
    let year = make_test_span("2", 12.5, 100.0, 8.0, 12.0);
    assert!(PdfDocument::should_insert_space(&tuan, &year));
}

// SEG-INDIC: clause punctuation hugs the preceding Brahmic-script word, so a
// wide post-syllable advance must not float a danda / comma / colon off as
// its own token. Latin keeps its spacing (no regression). ~keep
#[test]
fn test_should_insert_space_indic_clause_punct_hugs() {
    let beng = make_test_span("ী", 0.0, 100.0, 12.0, 12.0);
    let danda = make_test_span("।", 15.0, 100.0, 6.0, 12.0);
    assert!(!PdfDocument::should_insert_space(&beng, &danda));
    let deva = make_test_span("ी", 0.0, 100.0, 12.0, 12.0);
    let comma = make_test_span(",", 15.0, 100.0, 5.0, 12.0);
    assert!(!PdfDocument::should_insert_space(&deva, &comma));
    // Latin word + comma at the same gap STILL gets a space (Indic-scoped). ~keep
    let latin = make_test_span("word", 0.0, 100.0, 12.0, 12.0);
    let comma2 = make_test_span(",", 15.0, 100.0, 5.0, 12.0);
    assert!(PdfDocument::should_insert_space(&latin, &comma2));
}

// SEG-KO: a Hangul eojeol that wrapped mid-syllable rejoins with no break;
// an eojeol-boundary wrap (text already ends with a space) still separates. ~keep
#[test]
fn test_hangul_midword_line_wrap() {
    let prev = make_test_span("집고양", 480.0, 110.0, 36.0, 12.0);
    let next = make_test_span("이의", 50.0, 95.0, 24.0, 12.0);
    assert!(PdfDocument::hangul_midword_line_wrap("…집고양", &prev, &next));
    assert!(!PdfDocument::hangul_midword_line_wrap("…했다 ", &prev, &next));
    let latin = make_test_span("the", 50.0, 95.0, 24.0, 12.0);
    assert!(!PdfDocument::hangul_midword_line_wrap("…집고양", &prev, &latin));
}

/// Helper: build a minimal PDF whose single character maps to U+FB01 (LATIN SMALL
/// LIGATURE FI) via a ToUnicode CMap. This exercises the path where pdfium hands us
/// U+FB01 from the font's ToUnicode map and we must NOT expand it to "fi".
fn build_ligature_fi_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin\n\
                    12 dict begin\n\
                    begincmap\n\
                    /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
                    /CMapName /Adobe-Identity-UCS def\n\
                    /CMapType 2 def\n\
                    1 begincodespacerange\n\
                    <01> <01>\n\
                    endcodespacerange\n\
                    1 beginbfchar\n\
                    <01> <FB01>\n\
                    endbfchar\n\
                    endcmap\n\
                    CMapName currentdict /CMap defineresource pop\n\
                    end\n\
                    end\n";

    // Content stream: BT /F1 12 Tf 100 500 Td (\001) Tj ET ~keep
    let content = "BT /F1 12 Tf 100 500 Td (\\001) Tj ET\n";

    let mut out: Vec<u8> = Vec::new();
    let mut off: Vec<usize> = vec![0];

    out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

    macro_rules! push {
        ($body:expr) => {{
            off.push(out.len());
            let id = off.len() - 1;
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", id, $body).as_bytes());
        }};
    }

    push!("<< /Type /Catalog /Pages 2 0 R >>");
    push!("<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    push!(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
    ));
    push!(format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
             /Encoding << /Type /Encoding /Differences [1 /fi] >> \
             /ToUnicode 6 0 R >>"
    ));
    push!(format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content));
    push!(format!("<< /Length {} >>\nstream\n{}endstream", cmap.len(), cmap));

    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", off.len()).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for &o in &off[1..] {
        out.extend_from_slice(format!("{:010} 00000 n \n", o).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            off.len(),
            xref_offset
        )
        .as_bytes(),
    );
    out
}

/// A ToUnicode CMap that maps char 0x01 → U+FB01 (ﬁ) must produce the
/// ligature character in extract_text output — NOT the expanded "fi".
///
/// Before the fix, `extract_text` unconditionally calls
/// `get_ligature_components(ﬁ)` → "fi", discarding the font's own
/// ToUnicode intent. After the fix the ligature char is preserved.
#[test]
fn test_ligature_fi_preserved_in_extract_text() {
    let pdf = build_ligature_fi_pdf();
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_text(0).unwrap();
    assert!(
        text.contains('\u{FB01}'),
        "U+FB01 (ﬁ) must be preserved in extracted text; got: {text:?}"
    );
    assert!(
        !text.contains("fi") || text.contains('\u{FB01}'),
        "must not expand ﬁ → fi; got: {text:?}"
    );
}

#[test]
fn test_find_references_string_obj() {
    assert!(PdfDocument::find_references(&Object::String(b"hello".to_vec())).is_empty());
}

#[test]
fn test_find_references_real_obj() {
    assert!(PdfDocument::find_references(&Object::Real(std::f64::consts::PI)).is_empty());
}

#[test]
fn test_find_references_name_obj() {
    assert!(PdfDocument::find_references(&Object::Name("Test".to_string())).is_empty());
}

#[test]
fn test_find_references_deeply_nested() {
    let inner_ref = Object::Reference(ObjectRef::new(10, 0));
    let inner_arr = Object::Array(vec![inner_ref]);
    let mut dict = std::collections::HashMap::new();
    dict.insert("Key".to_string(), inner_arr);
    let refs = PdfDocument::find_references(&Object::Dictionary(dict));
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, 10);
}

#[test]
fn test_font_identity_hash_with_encoding_dict() {
    let mut font_dict = std::collections::HashMap::new();
    font_dict.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
    font_dict.insert("Subtype".to_string(), Object::Name("Type1".to_string()));
    let mut enc = std::collections::HashMap::new();
    enc.insert("Type".to_string(), Object::Name("Encoding".to_string()));
    font_dict.insert("Encoding".to_string(), Object::Dictionary(enc));
    assert_ne!(PdfDocument::font_identity_hash_cheap(&Object::Dictionary(font_dict)), 0);
}

#[test]
fn test_font_identity_hash_with_encoding_ref() {
    let mut font_dict = std::collections::HashMap::new();
    font_dict.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
    font_dict.insert("Encoding".to_string(), Object::Reference(ObjectRef::new(99, 0)));
    assert_ne!(PdfDocument::font_identity_hash_cheap(&Object::Dictionary(font_dict)), 0);
}

#[test]
fn test_font_identity_hash_tounicode_changes_hash() {
    let mut d1 = std::collections::HashMap::new();
    d1.insert("BaseFont".to_string(), Object::Name("Arial".to_string()));
    d1.insert("ToUnicode".to_string(), Object::Reference(ObjectRef::new(50, 0)));
    let h1 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(d1));

    let mut d2 = std::collections::HashMap::new();
    d2.insert("BaseFont".to_string(), Object::Name("Arial".to_string()));
    let h2 = PdfDocument::font_identity_hash_cheap(&Object::Dictionary(d2));
    assert_ne!(h1, h2);
}

#[test]
fn test_font_identity_hash_with_descendant_fonts() {
    let mut d = std::collections::HashMap::new();
    d.insert("BaseFont".to_string(), Object::Name("CIDFont".to_string()));
    d.insert("Subtype".to_string(), Object::Name("Type0".to_string()));
    d.insert(
        "DescendantFonts".to_string(),
        Object::Array(vec![Object::Reference(ObjectRef::new(20, 0))]),
    );
    assert_ne!(PdfDocument::font_identity_hash_cheap(&Object::Dictionary(d)), 0);
}

// Regression: two same-named, non-embedded simple fonts whose /Encoding are
// REFERENCES to different /Differences arrays must not share an identity
// hash. The cheap hash folds only a constant marker for a referenced
// /Encoding, so without folding the referenced encoding's CONTENT they
// collide and the second font decodes through the first's /Differences (a
// substitution-cipher scramble). font_identity_hash_with_descendants must
// distinguish them. ~keep
#[test]
fn test_font_identity_hash_folds_referenced_encoding_differences() {
    let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();

    let enc = |names: &[&str]| {
        let mut diffs = vec![Object::Integer(1)];
        diffs.extend(names.iter().map(|n| Object::Name((*n).to_string())));
        let mut d = std::collections::HashMap::new();
        d.insert("Type".to_string(), Object::Name("Encoding".to_string()));
        d.insert("Differences".to_string(), Object::Array(diffs));
        Object::Dictionary(d)
    };
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(100, 0), enc(&["T", "h", "i", "s"]));
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(101, 0), enc(&["one", "T", "h", "e"]));

    let font = |enc_ref: u32| {
        let mut f = std::collections::HashMap::new();
        f.insert("BaseFont".to_string(), Object::Name("Times-Roman".to_string()));
        f.insert("Subtype".to_string(), Object::Name("Type1".to_string()));
        f.insert("Encoding".to_string(), Object::Reference(ObjectRef::new(enc_ref, 0)));
        Object::Dictionary(f)
    };

    let h100 = doc.font_identity_hash_with_descendants(&font(100));
    let h101 = doc.font_identity_hash_with_descendants(&font(101));
    assert_ne!(
        h100, h101,
        "fonts with different referenced /Differences must not collide"
    );

    assert_eq!(
        doc.font_identity_hash_with_descendants(&font(100)),
        doc.font_identity_hash_with_descendants(&font(100)),
    );
}

#[test]
fn font_identity_hash_ignores_document_local_reference_numbers() {
    let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let cmap = Object::Stream {
        dict: std::collections::HashMap::new(),
        data: bytes::Bytes::from_static(b"same semantic cmap"),
    };
    doc.object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(100, 0), cmap.clone());
    doc.object_cache.lock_or_recover().insert(ObjectRef::new(200, 0), cmap);
    let font = |reference| {
        Object::Dictionary(std::collections::HashMap::from([
            ("BaseFont".to_string(), Object::Name("CIDFont+F1".to_string())),
            ("Subtype".to_string(), Object::Name("Type0".to_string())),
            ("ToUnicode".to_string(), Object::Reference(reference)),
        ]))
    };

    assert_eq!(
        doc.font_identity_hash_with_descendants(&font(ObjectRef::new(100, 0))),
        doc.font_identity_hash_with_descendants(&font(ObjectRef::new(200, 0))),
        "resolved semantic content, not a document-local object number, defines font identity"
    );
}

// Regression for F32: object numbers are document-local. Two PDFs can use
// the same `/Widths 100 0 R` reference for different width arrays, so the
// cross-document cache key must fold the referenced array's content. ~keep
#[test]
fn font_identity_hash_folds_referenced_simple_widths_content() {
    let document_with_widths = |widths: Vec<Object>| {
        let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
        doc.object_cache
            .lock_or_recover()
            .insert(ObjectRef::new(100, 0), Object::Array(widths));
        doc
    };
    let font = || {
        let mut dict = std::collections::HashMap::new();
        dict.insert("BaseFont".to_string(), Object::Name("Helvetica".to_string()));
        dict.insert("Subtype".to_string(), Object::Name("Type1".to_string()));
        dict.insert("Encoding".to_string(), Object::Name("WinAnsiEncoding".to_string()));
        dict.insert("FirstChar".to_string(), Object::Integer(65));
        dict.insert("LastChar".to_string(), Object::Integer(67));
        dict.insert("Widths".to_string(), Object::Reference(ObjectRef::new(100, 0)));
        Object::Dictionary(dict)
    };

    let narrow = document_with_widths(vec![Object::Integer(400), Object::Integer(400), Object::Integer(400)]);
    let wide = document_with_widths(vec![Object::Integer(700), Object::Integer(700), Object::Integer(700)]);

    assert_ne!(
        narrow.font_identity_hash_with_descendants(&font()),
        wide.font_identity_hash_with_descendants(&font()),
        "the same object id must not alias different referenced /Widths content across documents"
    );
}

// Regression for F32's Type0 variant: descendant `/W` arrays are commonly
// indirect, and their object numbers are no identity across documents. ~keep
#[test]
fn font_identity_hash_folds_referenced_descendant_widths_content() {
    let document_with_widths = |widths: Vec<Object>| {
        let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
        let mut descendant = std::collections::HashMap::new();
        descendant.insert("Subtype".to_string(), Object::Name("CIDFontType2".to_string()));
        descendant.insert("W".to_string(), Object::Reference(ObjectRef::new(200, 0)));
        descendant.insert(
            "CIDSystemInfo".to_string(),
            Object::Dictionary(std::collections::HashMap::from([
                ("Registry".to_string(), Object::String(b"Adobe".to_vec())),
                ("Ordering".to_string(), Object::String(b"Identity".to_vec())),
                ("Supplement".to_string(), Object::Integer(0)),
            ])),
        );
        doc.object_cache
            .lock_or_recover()
            .insert(ObjectRef::new(6, 0), Object::Dictionary(descendant));
        doc.object_cache
            .lock_or_recover()
            .insert(ObjectRef::new(200, 0), Object::Array(widths));
        doc
    };
    let font = || {
        let mut dict = std::collections::HashMap::new();
        dict.insert("BaseFont".to_string(), Object::Name("CIDFont+F1".to_string()));
        dict.insert("Subtype".to_string(), Object::Name("Type0".to_string()));
        dict.insert("Encoding".to_string(), Object::Name("Identity-H".to_string()));
        dict.insert(
            "DescendantFonts".to_string(),
            Object::Array(vec![Object::Reference(ObjectRef::new(6, 0))]),
        );
        Object::Dictionary(dict)
    };

    let narrow = document_with_widths(vec![
        Object::Integer(1),
        Object::Array(vec![Object::Integer(400), Object::Integer(400)]),
    ]);
    let wide = document_with_widths(vec![
        Object::Integer(1),
        Object::Array(vec![Object::Integer(900), Object::Integer(900)]),
    ]);

    assert_ne!(
        narrow.font_identity_hash_with_descendants(&font()),
        wide.font_identity_hash_with_descendants(&font()),
        "the same object id must not alias different referenced descendant /W content across documents"
    );
}

#[test]
fn font_identity_hash_resolves_indirect_descendant_array_and_descriptor_content() {
    let document_with_descriptor = |flags: i64, program: &'static [u8]| {
        let doc = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
        doc.object_cache.lock_or_recover().insert(
            ObjectRef::new(100, 0),
            Object::Array(vec![Object::Reference(ObjectRef::new(6, 0))]),
        );
        doc.object_cache.lock_or_recover().insert(
            ObjectRef::new(6, 0),
            Object::Dictionary(std::collections::HashMap::from([
                ("Subtype".to_string(), Object::Name("CIDFontType2".to_string())),
                (
                    "CIDSystemInfo".to_string(),
                    Object::Dictionary(std::collections::HashMap::from([
                        ("Registry".to_string(), Object::String(b"Adobe".to_vec())),
                        ("Ordering".to_string(), Object::String(b"Identity".to_vec())),
                        ("Supplement".to_string(), Object::Integer(0)),
                    ])),
                ),
                ("FontDescriptor".to_string(), Object::Reference(ObjectRef::new(7, 0))),
            ])),
        );
        doc.object_cache.lock_or_recover().insert(
            ObjectRef::new(7, 0),
            Object::Dictionary(std::collections::HashMap::from([
                ("Flags".to_string(), Object::Integer(flags)),
                ("FontFile2".to_string(), Object::Reference(ObjectRef::new(8, 0))),
            ])),
        );
        doc.object_cache.lock_or_recover().insert(
            ObjectRef::new(8, 0),
            Object::Stream {
                dict: std::collections::HashMap::new(),
                data: bytes::Bytes::from_static(program),
            },
        );
        doc
    };
    let font = || {
        Object::Dictionary(std::collections::HashMap::from([
            ("BaseFont".to_string(), Object::Name("CIDFont+F1".to_string())),
            ("Subtype".to_string(), Object::Name("Type0".to_string())),
            ("Encoding".to_string(), Object::Name("Identity-H".to_string())),
            ("DescendantFonts".to_string(), Object::Reference(ObjectRef::new(100, 0))),
        ]))
    };

    let first = document_with_descriptor(4, b"first font program");
    let second = document_with_descriptor(32, b"second font program");

    assert_ne!(
        first.font_identity_hash_with_descendants(&font()),
        second.font_identity_hash_with_descendants(&font()),
        "indirect DescendantFonts, FontDescriptor metrics, and font-program content must be resolved"
    );
}

#[test]
fn cyclic_and_oversized_font_reference_graphs_are_not_shared() {
    let cyclic = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    cyclic
        .object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(100, 0), Object::Reference(ObjectRef::new(100, 0)));
    let font = |reference| {
        Object::Dictionary(std::collections::HashMap::from([
            ("BaseFont".to_string(), Object::Name("Helvetica".to_string())),
            ("Subtype".to_string(), Object::Name("Type1".to_string())),
            ("Widths".to_string(), Object::Reference(reference)),
        ]))
    };

    assert!(
        !cyclic
            .font_identity_hash_details(&font(ObjectRef::new(100, 0)))
            .cacheable
    );

    let overdeep = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    for object_id in 100..=132 {
        overdeep.object_cache.lock_or_recover().insert(
            ObjectRef::new(object_id, 0),
            Object::Reference(ObjectRef::new(object_id + 1, 0)),
        );
    }
    overdeep
        .object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(133, 0), Object::Array(vec![Object::Integer(400)]));

    assert!(
        !overdeep
            .font_identity_hash_details(&font(ObjectRef::new(100, 0)))
            .cacheable
    );

    let overwide = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let mut references = Vec::with_capacity(FONT_IDENTITY_MAX_RESOLVED_REFERENCES + 1);
    for offset in 0..=FONT_IDENTITY_MAX_RESOLVED_REFERENCES {
        let object_id = 1000 + u32::try_from(offset).expect("reference cap fits u32");
        let reference = ObjectRef::new(object_id, 0);
        overwide
            .object_cache
            .lock_or_recover()
            .insert(reference, Object::Integer(400));
        references.push(Object::Reference(reference));
    }
    overwide
        .object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(999, 0), Object::Array(references));

    assert!(
        !overwide
            .font_identity_hash_details(&font(ObjectRef::new(999, 0)))
            .cacheable
    );
}

#[test]
fn encrypted_font_identity_is_not_shared_from_raw_ciphertext() {
    let mut document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    document.trailer = Object::Dictionary(std::collections::HashMap::from([(
        "Encrypt".to_string(),
        Object::Reference(ObjectRef::new(99, 0)),
    )]));
    let font = Object::Dictionary(std::collections::HashMap::from([
        ("BaseFont".to_string(), Object::Name("Helvetica".to_string())),
        ("Subtype".to_string(), Object::Name("Type1".to_string())),
    ]));

    assert!(!document.font_identity_hash_details(&font).cacheable);
}

#[test]
fn shared_font_stream_is_hashed_once_per_document() {
    let document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let stream_data = bytes::Bytes::from(vec![b'F'; 4096]);
    document.object_cache.lock_or_recover().insert(
        ObjectRef::new(100, 0),
        Object::Stream {
            dict: std::collections::HashMap::new(),
            data: stream_data.clone(),
        },
    );
    let font = |base_font: &str| {
        Object::Dictionary(std::collections::HashMap::from([
            ("BaseFont".to_string(), Object::Name(base_font.to_string())),
            ("Subtype".to_string(), Object::Name("Type0".to_string())),
            ("ToUnicode".to_string(), Object::Reference(ObjectRef::new(100, 0))),
        ]))
    };

    assert!(document.font_identity_hash_details(&font("FontA")).cacheable);
    assert!(document.font_identity_hash_details(&font("FontB")).cacheable);
    assert_eq!(document.font_identity_hashed_bytes(), stream_data.len());
}

#[test]
fn font_hash_byte_budget_disables_shared_identity_caches() {
    let document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    document.object_cache.lock_or_recover().insert(
        ObjectRef::new(100, 0),
        Object::Stream {
            dict: std::collections::HashMap::new(),
            data: bytes::Bytes::from(vec![b'F'; FONT_IDENTITY_MAX_HASHED_BYTES + 1]),
        },
    );
    let font = Object::Dictionary(std::collections::HashMap::from([
        ("BaseFont".to_string(), Object::Name("FontA".to_string())),
        ("Subtype".to_string(), Object::Name("Type0".to_string())),
        ("ToUnicode".to_string(), Object::Reference(ObjectRef::new(100, 0))),
    ]));

    assert!(!document.font_identity_hash_details(&font).cacheable);
    assert!(!document.font_identity_shared_cache_enabled());
    let cheap_font = Object::Dictionary(std::collections::HashMap::from([(
        "BaseFont".to_string(),
        Object::Name("Helvetica".to_string()),
    )]));
    assert!(!document.font_identity_hash_details(&cheap_font).cacheable);
}

#[test]
fn memoized_font_references_still_obey_per_root_reference_limit() {
    let document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let mut references = Vec::with_capacity(FONT_IDENTITY_MAX_RESOLVED_REFERENCES + 1);
    for offset in 0..=FONT_IDENTITY_MAX_RESOLVED_REFERENCES {
        let object_id = 1000 + u32::try_from(offset).expect("reference cap fits u32");
        let reference = ObjectRef::new(object_id, 0);
        document
            .object_cache
            .lock_or_recover()
            .insert(reference, Object::Integer(400));
        let font = Object::Dictionary(std::collections::HashMap::from([(
            "Widths".to_string(),
            Object::Reference(reference),
        )]));
        assert!(document.font_identity_hash_details(&font).cacheable);
        references.push(Object::Reference(reference));
    }
    document
        .object_cache
        .lock_or_recover()
        .insert(ObjectRef::new(999, 0), Object::Array(references));
    let font = Object::Dictionary(std::collections::HashMap::from([(
        "Widths".to_string(),
        Object::Reference(ObjectRef::new(999, 0)),
    )]));

    assert!(!document.font_identity_hash_details(&font).cacheable);
}

#[test]
fn memoized_font_reference_still_obeys_remaining_depth_budget() {
    let document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let shared_reference = ObjectRef::new(100, 0);
    let mut shared_object = Object::Integer(400);
    for _ in 0..12 {
        shared_object = Object::Dictionary(std::collections::HashMap::from([(
            "FontDescriptor".to_string(),
            shared_object,
        )]));
    }
    document
        .object_cache
        .lock_or_recover()
        .insert(shared_reference, shared_object);
    let shallow_font = Object::Dictionary(std::collections::HashMap::from([(
        "Widths".to_string(),
        Object::Reference(shared_reference),
    )]));
    assert!(document.font_identity_hash_details(&shallow_font).cacheable);

    let first_outer_reference = ObjectRef::new(200, 0);
    for object_id in 200..210 {
        let next_reference = if object_id == 209 {
            shared_reference
        } else {
            ObjectRef::new(object_id + 1, 0)
        };
        document.object_cache.lock_or_recover().insert(
            ObjectRef::new(object_id, 0),
            Object::Dictionary(std::collections::HashMap::from([(
                "FontDescriptor".to_string(),
                Object::Reference(next_reference),
            )])),
        );
    }
    let deep_font = Object::Dictionary(std::collections::HashMap::from([(
        "Widths".to_string(),
        Object::Reference(first_outer_reference),
    )]));

    assert!(!document.font_identity_hash_details(&deep_font).cacheable);
}

fn build_pdf_with_annotations(annot_objects: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<(usize, usize)> = Vec::new();

    let off1 = pdf.len();
    offsets.push((1, off1));
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    offsets.push((2, off2));
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let annot_refs: String = annot_objects
        .iter()
        .map(|(num, _)| format!("{} 0 R", num))
        .collect::<Vec<_>>()
        .join(" ");

    let off3 = pdf.len();
    offsets.push((3, off3));
    let page_str = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Annots [{}] >>\nendobj\n",
        annot_refs
    );
    pdf.extend_from_slice(page_str.as_bytes());

    for (obj_num, obj_data) in &annot_objects {
        let off = pdf.len();
        offsets.push((*obj_num, off));
        pdf.extend_from_slice(obj_data);
    }

    let max_obj = offsets.iter().map(|(n, _)| *n).max().unwrap_or(0);
    let xref_off = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", max_obj + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for obj_num in 1..=max_obj {
        if let Some((_, off)) = offsets.iter().find(|(n, _)| *n == obj_num) {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        } else {
            pdf.extend_from_slice(b"0000000000 65535 f \n");
        }
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            max_obj + 1,
            xref_off
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn test_annotation_freetext() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /FreeText /Contents (Hello from annotation) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_text(0).unwrap();
    assert!(text.contains("Hello from annotation"));
}

#[test]
fn test_annotation_text_type() {
    // Text (sticky-note) /Contents is reviewer popup comment text, not visible page
    // content — it must NOT appear in extract_text output (ISO 32000-1 §12.5.6.2). ~keep
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /Text /Contents (Sticky note) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("Sticky note"));
}

#[test]
fn test_annotation_stamp() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /Stamp /Contents (APPROVED) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().contains("APPROVED"));
}

#[test]
fn test_annotation_link() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /Link /Contents (Click here) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().contains("Click here"));
}

#[test]
fn test_annotation_highlight() {
    // Highlight annotation /Contents is a user comment on the highlighted
    // text — it is NOT page content and must NOT appear in extract_text output. ~keep
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /Highlight /Contents (Highlighted) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("Highlighted"));
}

#[test]
fn test_annotation_hidden_flag() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /FreeText /F 2 /Contents (Hidden) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("Hidden"));
}

#[test]
fn test_annotation_invisible_flag() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /FreeText /F 1 /Contents (Invisible) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("Invisible"));
}

#[test]
fn test_annotation_noview_flag() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /Text /F 32 /Contents (NoView) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("NoView"));
}

#[test]
fn test_annotation_unknown_subtype() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /CustomType /Contents (Custom) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().contains("Custom"));
}

#[test]
fn test_annotation_multiple() {
    // FreeText /Contents is visible page text; Text (sticky-note) /Contents is popup
    // comment — only FreeText should appear in extract_text output. ~keep
    let a1 = b"4 0 obj\n<< /Type /Annot /Subtype /FreeText /Contents (First) >>\nendobj\n".to_vec();
    let a2 = b"5 0 obj\n<< /Type /Annot /Subtype /Text /Contents (Second) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, a1), (5, a2)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_text(0).unwrap();
    assert!(text.contains("First"));
    assert!(!text.contains("Second"));
}

#[test]
fn test_annotation_no_subtype() {
    let annot = b"4 0 obj\n<< /Type /Annot /Contents (No subtype) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_text(0).unwrap().contains("No subtype"));
}

#[test]
fn test_annotation_widget_with_value() {
    let annot =
        b"4 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Tx /V (Field value) /Rect [72 700 272 720] >>\nendobj\n"
            .to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().contains("Field value"));
}

#[test]
fn test_resolve_references_boolean() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let resolved = doc.resolve_references(&Object::Boolean(true), 5).unwrap();
    assert!(matches!(resolved, Object::Boolean(true)));
}

#[test]
fn test_resolve_references_nested_dict_with_refs() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mut dict = std::collections::HashMap::new();
    dict.insert("CatalogRef".to_string(), Object::Reference(ObjectRef::new(1, 0)));
    dict.insert("Direct".to_string(), Object::Integer(42));
    let resolved = doc.resolve_references(&Object::Dictionary(dict), 3).unwrap();
    let rd = resolved.as_dict().unwrap();
    assert!(rd.get("CatalogRef").unwrap().as_dict().is_some());
    assert_eq!(rd.get("Direct").unwrap().as_integer(), Some(42));
}

#[test]
fn test_resolve_references_array_with_refs() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let arr = Object::Array(vec![Object::Reference(ObjectRef::new(1, 0)), Object::Integer(99)]);
    let resolved = doc.resolve_references(&arr, 3).unwrap();
    let ra = resolved.as_array().unwrap();
    assert!(ra[0].as_dict().is_some());
    assert_eq!(ra[1].as_integer(), Some(99));
}

#[test]
fn test_check_circular_refs_on_minimal_pdf() {
    // The minimal PDF has a page tree cycle:
    // Pages (2 0 R) -> Kids -> Page (3 0 R) -> Parent -> Pages (2 0 R)
    // The DFS cycle detector reports this as a cycle. ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let cycles = doc.check_for_circular_references();
    assert!(!cycles.is_empty());
}

#[test]
fn test_extract_text_graphics_only() {
    let pdf = build_minimal_pdf(b"q 1 0 0 1 0 0 cm 100 200 300 400 re S Q");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().is_empty());
}

#[test]
fn test_extract_text_page_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(100).is_err());
}

#[test]
fn test_extract_all_text_zero_pages() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_all_text().unwrap().is_empty());
}

#[test]
fn test_extract_spans_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_spans(999).is_err());
}

#[test]
fn test_extract_chars_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_chars(999).is_err());
}

#[test]
fn test_get_page_content_data_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.get_page_content_data(999).is_err());
}

#[test]
fn test_extract_paths_line() {
    let pdf = build_minimal_pdf(b"0 0 m 100 100 l S");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_paths(0).unwrap().is_empty());
}

#[test]
fn test_extract_paths_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_paths(999).is_err());
}

#[test]
fn test_extract_paths_curve() {
    let pdf = build_minimal_pdf(b"0 0 m 25 50 75 50 100 0 c S");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_paths(0).unwrap().is_empty());
}

#[test]
fn test_extract_paths_filled_rect() {
    let pdf = build_minimal_pdf(b"50 50 200 100 re f");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(!doc.extract_paths(0).unwrap().is_empty());
}

#[test]
fn test_extract_paths_in_rect_with_content() {
    let pdf = build_minimal_pdf(b"100 200 300 400 re S");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let region = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 612.0,
        height: 792.0,
    };
    assert!(!doc.extract_paths_in_rect(0, region).unwrap().is_empty());
}

#[test]
fn test_extract_images_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_images(999).is_err());
}

#[test]
fn test_mark_info_with_suspects() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true /Suspects true /UserProperties true >> >>\nendobj\n",
        );
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mi = doc.mark_info().unwrap();
    assert!(mi.marked);
    assert!(mi.suspects);
    assert!(mi.user_properties);
}

#[test]
fn test_page_count_exceeds_objects() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 999 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 1);
}

#[test]
fn test_page_count_rescued_when_count_is_zero_but_pages_exist() {
    // The motivating broken-/Count case: a `/Pages` node whose `/Count`
    // says 0 while its `/Kids` hold real pages. An ObjStm-packed `/Pages` tree
    // that the standard reader cannot resolve reaches `page_count` the same way
    // - `primary == Ok(0)`. Here the standard reader trusts the literal `/Count`
    // and returns 0, but `get_page` still walks `/Pages` -> `/Kids` and reaches
    // every page, so the rescue enumerates them.
    //
    // WITHOUT the rescue block this returns 0 (verified: reverting the
    // document.rs hunk makes this assertion fail with `0 != 3`); WITH it, 3. ~keep
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 0 >>\nendobj\n");
    let mut offs = vec![off1, off2];
    for n in 3..=5u32 {
        offs.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n",
                n
            )
            .as_bytes(),
        );
    }
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offs {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    // The standard /Count reader really does report 0 here, so the count of 3
    // comes entirely from the enumerator rescue (not from the primary path). ~keep
    assert_eq!(
        doc.get_page_count_standard().unwrap(),
        0,
        "fixture must drive the standard reader to 0"
    );
    assert_eq!(doc.page_count().unwrap(), 3, "rescue must enumerate the real pages");
}

#[test]
fn test_deeply_nested_page_tree() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 595 842] /Resources << >> >>\nendobj\n",
    );
    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 /Parent 2 0 R >>\nendobj\n");
    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /Parent 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert_eq!(doc.page_count().unwrap(), 1);
    let page = doc.get_page(0).unwrap();
    assert!(page.as_dict().unwrap().contains_key("MediaBox"));
}

#[test]
fn test_populate_page_cache_sequential() {
    let pdf = build_multi_page_pdf(5);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    for i in 0..5 {
        assert!(doc.get_page(i).unwrap().as_dict().is_some());
    }
}

#[test]
fn test_get_page_ref_multi_page() {
    let pdf = build_multi_page_pdf(3);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let r0 = doc.get_page_ref(0).unwrap();
    let r1 = doc.get_page_ref(1).unwrap();
    let r2 = doc.get_page_ref(2).unwrap();
    assert_ne!(r0.id, r1.id);
    assert_ne!(r1.id, r2.id);
}

#[test]
fn test_page_content_indirect_array() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << >> >>\nendobj\n",
    );
    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n[5 0 R 6 0 R]\nendobj\n");
    let c1 = b"q";
    let off5 = pdf.len();
    pdf.extend_from_slice(format!("5 0 obj\n<< /Length {} >>\nstream\n", c1.len()).as_bytes());
    pdf.extend_from_slice(c1);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let c2 = b"Q";
    let off6 = pdf.len();
    pdf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", c2.len()).as_bytes());
    pdf.extend_from_slice(c2);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off6).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let data = doc.get_page_content_data(0).unwrap();
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("q"));
    assert!(text.contains("Q"));
}

#[test]
fn corrupt_optional_content_stream_warns_with_operation_identity() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << >> >>\nendobj\n",
    );
    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n[5 0 R 6 0 R]\nendobj\n");
    let corrupt = b"This is not zlib compressed data";
    let off5 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            corrupt.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(corrupt);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let valid = b"Q";
    let off6 = pdf.len();
    pdf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", valid.len()).as_bytes());
    pdf.extend_from_slice(valid);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for offset in [off1, off2, off3, off4, off5, off6] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let (result, events) = capture_events(|| doc.get_page_content_data(0));

    assert_eq!(result.unwrap().as_slice(), b"Q\n");
    let warning = events.iter().find(|event| {
        event.level == Level::WARN
            && event.target == crate::LOG_TARGET_ROOT
            && event.fields.get("operation").map(String::as_str) == Some("decode_optional_page_content")
    });
    let warning = warning.unwrap_or_else(|| panic!("missing optional-content warning: {events:#?}"));
    assert_eq!(warning.fields.get("page_index").map(String::as_str), Some("0"));
    assert_eq!(
        warning.fields.get("error_code").map(String::as_str),
        Some("decode_error")
    );
    assert!(!warning.fields.contains_key("error"));
    assert!(events.iter().all(|event| event.level != Level::ERROR));
}

#[test]
fn corrupt_mandatory_xref_stream_errors_at_open_boundary() {
    let corrupt = b"This is not zlib compressed data";
    let mut pdf = b"%PDF-1.5\n".to_vec();
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!(
            "1 0 obj\n<< /Type /XRef /Size 2 /W [1 2 1] /Length {} /Filter /FlateDecode >>\nstream\n",
            corrupt.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(corrupt);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    let (result, events) = capture_events(|| PdfDocument::from_bytes(pdf));

    assert!(result.is_err(), "a corrupt mandatory xref stream must fail open");
    let failures: Vec<_> = events
        .iter()
        .filter(|event| {
            event.level == Level::ERROR
                && event.target == format!("{}::document", crate::LOG_TARGET_ROOT)
                && event.fields.contains_key("error_code")
        })
        .collect();
    assert_eq!(failures.len(), 1, "expected one boundary error: {events:#?}");
    assert_eq!(
        events.iter().filter(|event| event.level == Level::ERROR).count(),
        1,
        "fatal xref failure must emit exactly one ERROR: {events:#?}"
    );
    assert_eq!(
        failures[0].fields.get("error_code").map(String::as_str),
        Some("invalid_pdf")
    );
    let reconstruction = events.iter().find(|event| {
        event.level == Level::WARN
            && event.target == crate::LOG_TARGET_ROOT
            && event.fields.get("operation").map(String::as_str) == Some("reconstruct_xref")
    });
    let reconstruction = reconstruction.unwrap_or_else(|| panic!("missing reconstruction context: {events:#?}"));
    assert!(reconstruction.fields.contains_key("primary_error_code"));
    assert!(reconstruction.fields.contains_key("reconstruction_error_code"));
}

#[test]
fn xref_recovery_tracing_does_not_expose_document_bytes() {
    const CONFIDENTIAL_MARKER: &str = "CONFIDENTIAL_ENTERPRISE_PAYLOAD_7f40c6";
    let confidential_content = CONFIDENTIAL_MARKER.repeat(4096);
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let object_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{object_offset:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Root ${confidential_content} >>\n").as_bytes());
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    let (result, events) = capture_events(|| PdfDocument::from_bytes(pdf));
    assert!(result.is_ok(), "reconstruction should recover the malformed trailer");
    let parse_failures: Vec<_> = events
        .iter()
        .filter(|event| {
            event.level == Level::WARN
                && event.target == format!("{}::document", crate::LOG_TARGET_ROOT)
                && event.fields.get("error_code").map(String::as_str) == Some("parse_error")
                && event.fields.get("message").map(String::as_str)
                    == Some("regular xref parsing failed; attempting reconstruction")
        })
        .collect();
    assert_eq!(
        parse_failures.len(),
        1,
        "expected exactly one regular-xref recovery warning: {events:#?}"
    );
    let captured = format!("{events:?}");
    let confidential_bytes = CONFIDENTIAL_MARKER
        .as_bytes()
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    assert!(
        !captured.contains(CONFIDENTIAL_MARKER) && !captured.contains(&confidential_bytes),
        "recovery telemetry exposed attacker-controlled document bytes: {captured}"
    );
    assert!(
        events
            .iter()
            .flat_map(|event| event.fields.values())
            .all(|value| value.len() <= 256),
        "recovery telemetry emitted an unbounded field: {events:#?}"
    );
}

#[test]
fn malformed_object_header_warning_does_not_expose_header_bytes() {
    const CONFIDENTIAL_MARKER: &str = "CONFIDENTIAL_OBJECT_HEADER_182f13";
    let mut pdf = build_minimal_pdf(b"");
    let marker_offset = pdf.len();
    pdf.extend_from_slice(CONFIDENTIAL_MARKER.as_bytes());
    let document = PdfDocument::from_bytes(pdf).unwrap();

    let (result, events) =
        capture_events(|| document.load_uncompressed_object_impl(ObjectRef::new(999, 0), marker_offset as u64, true));

    assert!(result.is_err());
    let warnings: Vec<_> = events.iter().filter(|event| event.level == Level::WARN).collect();
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one malformed-header warning: {events:#?}"
    );
    assert_eq!(warnings[0].target, format!("{}::document", crate::LOG_TARGET_ROOT));
    assert_eq!(
        warnings[0].fields.get("operation").map(String::as_str),
        Some("load_uncompressed_object")
    );
    assert_eq!(
        warnings[0].fields.get("error_code").map(String::as_str),
        Some("malformed_object_header")
    );
    assert!(
        !format!("{events:?}").contains(CONFIDENTIAL_MARKER),
        "object-header telemetry exposed attacker-controlled bytes: {events:#?}"
    );
}

#[test]
fn encryption_recovery_warning_does_not_expose_parser_error_text() {
    const CONFIDENTIAL_MARKER: &str = "CONFIDENTIAL_ENCRYPTION_ENTRY_43fb0d";
    let error = Error::InvalidPdf(CONFIDENTIAL_MARKER.repeat(4096));

    let (_, events) = capture_events(|| trace_recoverable_pdf_error("initialize_encryption", &error));

    let warnings: Vec<_> = events.iter().filter(|event| event.level == Level::WARN).collect();
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one encryption warning: {events:#?}"
    );
    assert_eq!(warnings[0].target, crate::LOG_TARGET_ROOT);
    assert_eq!(
        warnings[0].fields.get("operation").map(String::as_str),
        Some("initialize_encryption")
    );
    assert_eq!(
        warnings[0].fields.get("error_code").map(String::as_str),
        Some("invalid_pdf")
    );
    assert!(
        !format!("{events:?}").contains(CONFIDENTIAL_MARKER),
        "encryption telemetry exposed attacker-controlled error text: {events:#?}"
    );
}

#[test]
fn unresolved_encryption_references_emit_one_counted_warning() {
    let encryption_dictionary = HashMap::from([
        ("V".to_string(), Object::Reference(ObjectRef::new(900, 0))),
        ("R".to_string(), Object::Reference(ObjectRef::new(901, 0))),
    ]);

    let (_, events) = capture_events(|| {
        resolve_encrypt_dictionary_references(&encryption_dictionary, |_| {
            Err(Error::InvalidPdf("CONFIDENTIAL_ENCRYPT_REFERENCE".repeat(4096)))
        })
    });

    let warnings: Vec<_> = events
        .iter()
        .filter(|event| {
            event.level == Level::WARN
                && event.target == crate::LOG_TARGET_ROOT
                && event.fields.get("operation").map(String::as_str) == Some("resolve_encrypt_reference")
        })
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "expected one aggregate encryption warning: {events:#?}"
    );
    assert_eq!(
        warnings[0].fields.get("error_code").map(String::as_str),
        Some("unresolved_reference")
    );
    assert_eq!(warnings[0].fields.get("skipped_count").map(String::as_str), Some("2"));
    assert!(!warnings[0].fields.contains_key("error"));
    assert!(!format!("{events:?}").contains("CONFIDENTIAL_ENCRYPT_REFERENCE"));
}

#[test]
fn malformed_object_stream_is_parsed_once_for_multiple_missing_references() {
    use crate::xref::XRefEntry;

    let mut pdf = b"%PDF-1.5\n".to_vec();
    let catalog_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let pages_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let object_stream_offset = pdf.len();
    pdf.extend_from_slice(
        b"10 0 obj\n<< /Type /ObjStm /N 2 /First 10 /Length 14 >>\nstream\n30 0 31 3 42 $\nendstream\nendobj\n",
    );
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 11\n0000000000 65535 f \n");
    for object_id in 1..=10 {
        let offset = match object_id {
            1 => catalog_offset,
            2 => pages_offset,
            10 => object_stream_offset,
            _ => 0,
        };
        let state = if offset == 0 { 'f' } else { 'n' };
        pdf.extend_from_slice(format!("{offset:010} 00000 {state} \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());

    let mut document = PdfDocument::from_bytes(pdf).unwrap();
    document.xref.add_entry(20, XRefEntry::compressed(10, 0));
    document.xref.add_entry(21, XRefEntry::compressed(10, 1));
    document.object_stream_cache = Mutex::new(BoundedObjectStreamCache::new(1));

    let ((first, second), events) = capture_events(|| {
        (
            document.load_compressed_object(ObjectRef::new(20, 0), 10, 0),
            document.load_compressed_object(ObjectRef::new(21, 0), 10, 1),
        )
    });
    assert_eq!(first.unwrap(), Object::Null);
    assert_eq!(second.unwrap(), Object::Null);
    let warnings: Vec<_> = events
        .iter()
        .filter(|event| {
            event.level == Level::WARN
                && event.target == crate::LOG_TARGET_ROOT
                && event.fields.get("operation").map(String::as_str) == Some("parse_object_stream")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "object stream must be parsed once: {events:#?}");
    assert_eq!(
        warnings[0].fields,
        BTreeMap::from([
            ("error_code".to_string(), "invalid_embedded_object".to_string()),
            ("invalid_offset_count".to_string(), "0".to_string()),
            ("message".to_string(), "object stream entries were skipped".to_string()),
            ("operation".to_string(), "parse_object_stream".to_string()),
            ("parse_failure_count".to_string(), "1".to_string()),
            ("skipped_count".to_string(), "1".to_string()),
        ])
    );
    assert_eq!(document.object_stream_cache.lock_or_recover().map.len(), 0);
    assert_eq!(document.object_stream_telemetry_seen.lock_or_recover().seen.len(), 1);
}

#[test]
fn clean_object_stream_does_not_consume_recovery_marker_budget() {
    let document = PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap();
    let stream = Object::Stream {
        dict: HashMap::from([
            ("Type".to_string(), Object::Name("ObjStm".to_string())),
            ("N".to_string(), Object::Integer(1)),
            ("First".to_string(), Object::Integer(5)),
        ]),
        data: bytes::Bytes::from_static(b"30 0 42"),
    };
    let outcome = crate::objstm::parse_object_stream_with_decryption_outcome(&stream, None, 0, 0).unwrap();

    document.trace_object_stream_recovery_once(10, &outcome);

    let marker = document.object_stream_telemetry_seen.lock_or_recover();
    assert_eq!(marker.seen.len(), 0);
    assert!(!marker.saturated);
}

fn build_catalog_test_pdf(catalog: &[u8], pages: &[u8], extra_objects: &[(u32, &[u8])]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let max_object_id = extra_objects.iter().map(|(id, _)| *id).max().unwrap_or(2).max(2);
    let mut offsets = vec![None; max_object_id as usize + 1];
    for (object_id, body) in std::iter::once((1u32, catalog))
        .chain(std::iter::once((2u32, pages)))
        .chain(extra_objects.iter().copied())
    {
        offsets[object_id as usize] = Some(pdf.len());
        pdf.extend_from_slice(format!("{object_id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.into_iter().skip(1) {
        match offset {
            Some(offset) => pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes()),
            None => pdf.extend_from_slice(b"0000000000 00000 f \n"),
        }
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            max_object_id + 1
        )
        .as_bytes(),
    );
    pdf
}

fn corrupt_third_object_header(mut pdf: Vec<u8>) -> Vec<u8> {
    const HEADER: &[u8] = b"3 0 obj";
    let position = pdf
        .windows(HEADER.len())
        .position(|window| window == HEADER)
        .expect("test object header must exist");
    pdf[position..position + HEADER.len()].copy_from_slice(b"X 0 bad");
    pdf
}

#[test]
fn output_intent_and_page_tree_warnings_hide_attacker_controlled_details() {
    const SECRET_FILTER: &str = "CONFIDENTIAL_OUTPUT_FILTER_7b91";
    const SECRET_PROFILE: &str = "CONFIDENTIAL_OUTPUT_PROFILE_34c0";
    const SECRET_PAGE_TYPE: &str = "CONFIDENTIAL_PAGE_TYPE_a29e";
    let pages = b"<< /Type /Pages /Kids [] /Count 0 >>";
    let filtered_profile_body = format!("<< /N 4 /Filter /{SECRET_FILTER} /Length 1 >>\nstream\nx\nendstream");
    let undecodable_profile = PdfDocument::from_bytes(build_catalog_test_pdf(
        b"<< /Type /Catalog /Pages 2 0 R /OutputIntents [<< /DestOutputProfile 3 0 R >>] >>",
        pages,
        &[(3, filtered_profile_body.as_bytes())],
    ))
    .unwrap();
    let invalid_profile_body = format!(
        "<< /N 4 /Length {} >>\nstream\n{SECRET_PROFILE}\nendstream",
        SECRET_PROFILE.len()
    );
    let invalid_profile = PdfDocument::from_bytes(build_catalog_test_pdf(
        b"<< /Type /Catalog /Pages 2 0 R /OutputIntents [<< /DestOutputProfile 3 0 R >>] >>",
        pages,
        &[(3, invalid_profile_body.as_bytes())],
    ))
    .unwrap();
    let malformed_indirect_body = format!("<< /{SECRET_PROFILE}");
    let malformed_entry = PdfDocument::from_bytes(corrupt_third_object_header(build_catalog_test_pdf(
        b"<< /Type /Catalog /Pages 2 0 R /OutputIntents [3 0 R] >>",
        pages,
        &[(3, malformed_indirect_body.as_bytes())],
    )))
    .unwrap();
    let malformed_profile = PdfDocument::from_bytes(corrupt_third_object_header(build_catalog_test_pdf(
        b"<< /Type /Catalog /Pages 2 0 R /OutputIntents [<< /DestOutputProfile 3 0 R >>] >>",
        pages,
        &[(3, malformed_indirect_body.as_bytes())],
    )))
    .unwrap();
    let unknown_page_catalog = b"<< /Type /Catalog /Pages 2 0 R >>";
    let unknown_page_body = format!("<< /Type /{SECRET_PAGE_TYPE} /Kids [] /Count 0 >>");
    let unknown_page = PdfDocument::from_bytes(build_catalog_test_pdf(
        unknown_page_catalog,
        unknown_page_body.as_bytes(),
        &[],
    ))
    .unwrap();

    let (_, events) = capture_events(|| {
        assert!(malformed_entry.output_intent_cmyk_profile().is_none());
        assert!(malformed_profile.output_intent_cmyk_profile().is_none());
        assert!(undecodable_profile.output_intent_cmyk_profile().is_none());
        assert!(invalid_profile.output_intent_cmyk_profile().is_none());
        assert_eq!(unknown_page.count_pages_recursive(ObjectRef::new(2, 0), 0).unwrap(), 0);
    });
    let warnings: Vec<_> = events
        .iter()
        .filter(|event| event.level == Level::WARN && event.target == crate::LOG_TARGET_ROOT)
        .collect();
    for (operation, error_code) in [
        ("load_output_intent_entry", "parse_error"),
        ("load_output_intent_profile", "parse_error"),
        ("decode_output_intent_profile", "unsupported_filter"),
        ("parse_output_intent_profile", "invalid_icc_profile"),
        ("traverse_page_tree", "unknown_node_type"),
    ] {
        assert_eq!(
            warnings
                .iter()
                .filter(|event| {
                    event.fields.get("operation").map(String::as_str) == Some(operation)
                        && event.fields.get("error_code").map(String::as_str) == Some(error_code)
                })
                .count(),
            1,
            "missing exact {operation}/{error_code} warning: {events:#?}"
        );
    }
    let rendered = format!("{events:?}");
    assert!(!rendered.contains(SECRET_FILTER));
    assert!(!rendered.contains(SECRET_PROFILE));
    assert!(!rendered.contains(SECRET_PAGE_TYPE));
}

#[test]
fn test_get_page_content_data_null_contents() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents null /Resources << >> >>\nendobj\n",
    );
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.get_page_content_data(0).unwrap().is_empty());
}

#[test]
fn test_scan_for_object_finds_missing() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n");
    let _off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /Type /Metadata /Subtype /XML >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let obj = doc.load_object(ObjectRef::new(5, 0)).unwrap();
    assert!(obj.as_dict().is_some());
}

#[test]
fn test_load_object_missing_returns_null_simple() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(matches!(doc.load_object(ObjectRef::new(999, 0)).unwrap(), Object::Null));
}

#[test]
fn test_decode_stream_with_encryption_non_null() {
    let pdf = build_minimal_pdf(b"BT (Hello) Tj ET");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let stream_obj = doc.load_object(ObjectRef::new(4, 0)).unwrap();
    assert!(
        doc.decode_stream_with_encryption(&stream_obj, ObjectRef::new(4, 0))
            .is_ok()
    );
}

#[test]
fn test_load_fonts_public_empty_resources() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mut ext = crate::extractors::TextExtractor::new();
    assert!(
        doc.load_fonts_public(&Object::Dictionary(std::collections::HashMap::new()), &mut ext)
            .is_ok()
    );
}

#[test]
fn test_load_fonts_public_resources_not_dict() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mut ext = crate::extractors::TextExtractor::new();
    assert!(doc.load_fonts_public(&Object::Integer(42), &mut ext).is_ok());
}

#[test]
fn test_is_form_xobject_from_cache() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let _ = doc.load_object(ObjectRef::new(1, 0)).unwrap();
    assert!(!doc.is_form_xobject(ObjectRef::new(1, 0)));
}

#[test]
fn test_find_substring_middle() {
    assert_eq!(find_substring(b"Hello World", b"lo W"), Some(3));
}

#[test]
fn test_find_substring_full_match() {
    assert_eq!(find_substring(b"ABC", b"ABC"), Some(0));
}

#[test]
fn test_find_substring_needle_longer() {
    assert_eq!(find_substring(b"AB", b"ABCD"), None);
}

#[test]
fn test_parse_header_lenient_no_header() {
    let mut cursor = Cursor::new(vec![0xABu8; 100]);
    let (major, minor, _) = parse_header(&mut cursor, true).unwrap();
    assert_eq!((major, minor), (1, 4));
}

#[test]
fn test_parse_version_lenient_version_0_0() {
    let header = *b"%PDF-0.0";
    assert_eq!(parse_version_from_header(&header, true).unwrap(), (1, 4));
}

#[test]
fn test_parse_trailer_empty_input() {
    assert!(parse_trailer(&mut Cursor::new(b"")).is_err());
}

#[test]
fn test_apply_intelligent_text_processing_fl_ligature_preserved() {
    // Same as ﬁ: ﬂ (U+FB02) must be preserved, not expanded to "fl". ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let spans = vec![make_test_span("\u{FB02}oor", 0.0, 0.0, 50.0, 12.0)];
    let result = doc.apply_intelligent_text_processing(spans);
    assert!(
        result[0].text.contains('\u{FB02}'),
        "ﬂ must be preserved, got: {:?}",
        result[0].text
    );
}

#[test]
fn test_apply_intelligent_text_processing_ocr_font() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let mut span = make_test_span("Test  Text", 0.0, 0.0, 100.0, 12.0);
    span.font_name = "OCR".to_string();
    let result = doc.apply_intelligent_text_processing(vec![span]);
    assert!(!result[0].text.contains("  "));
}

#[test]
fn test_extract_spans_with_config_adaptive() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(
        doc.extract_spans_with_config(0, crate::extractors::SpanMergingConfig::adaptive())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_extract_spans_with_config_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(
        doc.extract_spans_with_config(999, crate::extractors::SpanMergingConfig::default())
            .is_err()
    );
}

#[test]
fn test_image_format_debug() {
    assert_eq!(format!("{:?}", ImageFormat::Png), "Png");
    assert_eq!(format!("{:?}", ImageFormat::Jpeg), "Jpeg");
}

#[test]
fn test_may_contain_text_bt_with_newline() {
    assert!(PdfDocument::may_contain_text(b"\nBT\n"));
}

#[test]
fn test_may_contain_text_do_with_bracket() {
    assert!(PdfDocument::may_contain_text(b"]Do["));
}

#[test]
fn test_may_contain_text_single_b() {
    assert!(!PdfDocument::may_contain_text(b"B"));
}

#[test]
fn test_may_contain_text_single_d() {
    assert!(!PdfDocument::may_contain_text(b"D"));
}

#[test]
fn test_multiline_object_header() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1\n0\nobj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.catalog().unwrap().as_dict().is_some());
}

#[test]
fn test_object_content_on_same_line() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.catalog().unwrap().as_dict().is_some());
}

#[test]
fn test_open_pdf_version_2_0() {
    let mut pdf = b"%PDF-2.0\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    assert_eq!(PdfDocument::from_bytes(pdf).unwrap().version(), (2, 0));
}

#[test]
fn test_extract_text_annotations_only() {
    let annot = b"4 0 obj\n<< /Type /Annot /Subtype /FreeText /Contents (Only annotation) >>\nendobj\n".to_vec();
    let pdf = build_pdf_with_annotations(vec![(4, annot)]);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_text(0).unwrap().contains("Only annotation"));
}

#[test]
fn test_parse_string_value_static_boolean() {
    assert!(PdfDocument::parse_string_value_static(Some(&Object::Boolean(true))).is_none());
}

#[test]
fn test_parse_string_value_static_array() {
    assert!(PdfDocument::parse_string_value_static(Some(&Object::Array(vec![]))).is_none());
}

#[test]
#[allow(deprecated)]
fn test_page_count_u32_zero_pages() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());
    assert_eq!(PdfDocument::from_bytes(pdf).unwrap().page_count_u32(), 0);
}

/// Regression test: validate_object_at_offset must return true for
/// compressed (type 2) xref entries. Previously, it treated the object
/// stream number as a byte offset, sought to a random location,
/// returned false — triggering a full-file xref reconstruction that took
/// 35+ seconds on large PDFs.
#[test]
fn test_validate_compressed_xref_entry() {
    use crate::xref::{CrossRefTable, XRefEntry, XRefEntryType};

    let mut xref = CrossRefTable::new();
    // Add a compressed entry: object 5 lives inside object stream 10, at index 3 ~keep
    xref.entries.insert(
        5,
        XRefEntry {
            entry_type: XRefEntryType::Compressed,
            offset: 10,
            generation: 3,
            in_use: true,
        },
    );

    let data = b"%PDF-1.7\n%%EOF\n";
    let mut cursor = Cursor::new(data.to_vec());
    let obj_ref = ObjectRef { id: 5, generation: 0 };

    // Must return true — compressed objects are valid by virtue of being in the xref ~keep
    assert!(validate_object_at_offset(&mut cursor, &xref, obj_ref));
}

#[test]
fn test_reading_order_enum_default() {
    let order = ReadingOrder::default();
    assert_eq!(order, ReadingOrder::TopToBottom);
}

#[test]
fn test_reading_order_enum_variants() {
    assert_ne!(ReadingOrder::TopToBottom, ReadingOrder::ColumnAware);
    let a = ReadingOrder::ColumnAware;
    let b = a;
    assert_eq!(a, b);
}

/// Verify that ColumnAware reading order reads column 1 fully before column 2.
///
/// Layout:
/// ```text
///   Left col (x=10) Right col (x=200)
///   +-----------+ +-----------+
///   | L1 (y=700)| | R1 (y=700)|
///   | L2 (y=680)| | R2 (y=680)|
///   | L3 (y=660)| | R3 (y=660)|
///   +-----------+ +-----------+
/// ```
/// Expected ColumnAware order: L1, L2, L3, R1, R2, R3
/// TopToBottom order would interleave: L1, R1, L2, R2, L3, R3
#[test]
fn test_column_aware_reads_column1_before_column2() {
    use crate::geometry::Rect;
    use crate::layout::{Color, FontWeight, TextSpan};
    use crate::pipeline::reading_order::{ReadingOrderContext as ROContext, ReadingOrderStrategy, XYCutStrategy};

    fn make_span(label: &str, x: f32, y: f32) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: label.to_string(),
            bbox: Rect::new(x, y, 80.0, 12.0),
            font_size: 12.0,
            font_name: "Test".to_string(),
            font_weight: FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: Color { r: 0.0, g: 0.0, b: 0.0 },
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    let spans = vec![
        make_span("L1", 10.0, 700.0),
        make_span("R1", 200.0, 700.0),
        make_span("L2", 10.0, 680.0),
        make_span("R2", 200.0, 680.0),
        make_span("L3", 10.0, 660.0),
        make_span("R3", 200.0, 660.0),
    ];

    let strategy = XYCutStrategy::new();
    let context = ROContext::new();
    let ordered = strategy.apply(spans, &context).expect("XYCut should not fail");
    let labels: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

    assert_eq!(
        labels,
        vec!["L1", "L2", "L3", "R1", "R2", "R3"],
        "ColumnAware should read left column fully before right column"
    );
}

/// Regression test: a page with only 2 spans per column
/// (4 spans total) is below `min_spans_for_split` (5), so it never
/// reaches the geometric column-split logic the 6-span test above
/// exercises — every statistical prose/table classifier
/// (`classify_region_kind`, `detect_two_column_prose`,
/// `detect_narrow_gutter_prose`) also has its own internal minimum-span
/// floor (6/8/24) far above 4, so none of them can classify this page
/// either. Before the fix, the base case fell back to a flat
/// Y-then-X sort, interleaving the two columns (L1, R1, L2, R2)
/// instead of reading each column through.
///
/// A pure geometric gutter check can't distinguish this from a 2x2
/// table at this scale (see `test_column_aware_sparse_2x2_table_stays_row_major`
/// below), so the fix defers to content-stream emission order when a
/// clean gutter exists — PDFium parity per the issue's own cross-tool
/// probe. This fixture's `sequence` mirrors the exact reporter's
/// repro (`reportlab` draws the whole left column, then the whole
/// right column): L1, L2, R1, R2.
#[test]
fn test_column_aware_sparse_two_column_follows_stream_order() {
    use crate::geometry::Rect;
    use crate::layout::{Color, FontWeight, TextSpan};
    use crate::pipeline::reading_order::{ReadingOrderContext as ROContext, ReadingOrderStrategy, XYCutStrategy};

    fn make_span(label: &str, x: f32, y: f32, sequence: usize) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            mirrored: false,
            page_rotation_applied: 0,
            artifact_type: None,
            text: label.to_string(),
            bbox: Rect::new(x, y, 80.0, 12.0),
            font_size: 12.0,
            font_name: "Test".to_string(),
            font_weight: FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: Color { r: 0.0, g: 0.0, b: 0.0 },
            mcid: None,
            mcid_scope: None,
            sequence,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
        }
    }

    // Column-major stream order, matching a two-column-prose generator
    // that fills the left text box then the right one — exactly the
    // reporter's `reportlab` repro. ~keep
    let spans = vec![
        make_span("L1", 10.0, 700.0, 0),
        make_span("L2", 10.0, 680.0, 1),
        make_span("R1", 200.0, 700.0, 2),
        make_span("R2", 200.0, 680.0, 3),
    ];

    let strategy = XYCutStrategy::new();
    let context = ROContext::new();
    let ordered = strategy.apply(spans, &context).expect("XYCut should not fail");
    let labels: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

    assert_eq!(
        labels,
        vec!["L1", "L2", "R1", "R2"],
        "a sparse 2-column page below min_spans_for_split must follow \
             content-stream order (column-major here), not interleave the \
             columns via a flat Y-then-X sort"
    );
}

/// Companion to the test above: a genuine 2x2 table emitted **row-major**
/// in-stream (the common table-generator pattern — draw row 1's cells
/// left-to-right, then row 2's) must stay row-major. The same clean
/// gutter exists between the two columns as in the prose case above —
/// nothing in this codebase can geometrically tell the two apart at
/// 4-span scale — so the fix's content-stream-order fallback is
/// correct for *both* shapes precisely because it never has to decide
/// between them: it just preserves however the source authored it.
#[test]
fn test_column_aware_sparse_2x2_table_stays_row_major() {
    use crate::geometry::Rect;
    use crate::layout::{Color, FontWeight, TextSpan};
    use crate::pipeline::reading_order::{ReadingOrderContext as ROContext, ReadingOrderStrategy, XYCutStrategy};

    fn make_cell(label: &str, x: f32, y: f32, sequence: usize) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            mirrored: false,
            page_rotation_applied: 0,
            artifact_type: None,
            text: label.to_string(),
            bbox: Rect::new(x, y, 80.0, 12.0),
            font_size: 12.0,
            font_name: "Test".to_string(),
            font_weight: FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: Color { r: 0.0, g: 0.0, b: 0.0 },
            mcid: None,
            mcid_scope: None,
            sequence,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
        }
    }

    let spans = vec![
        make_cell("R1C1", 10.0, 700.0, 0),
        make_cell("R1C2", 200.0, 700.0, 1),
        make_cell("R2C1", 10.0, 680.0, 2),
        make_cell("R2C2", 200.0, 680.0, 3),
    ];

    let strategy = XYCutStrategy::new();
    let context = ROContext::new();
    let ordered = strategy.apply(spans, &context).expect("XYCut should not fail");
    let labels: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

    assert_eq!(
        labels,
        vec!["R1C1", "R1C2", "R2C1", "R2C2"],
        "a row-major-emitted 2x2 table must stay row-major, not be \
             reshuffled into a column-major read order"
    );
}

/// Build a span with explicit width and text (for corridor-geometry tests).
#[cfg(test)]
fn corridor_span(text: &str, x: f32, y: f32, w: f32) -> crate::layout::TextSpan {
    use crate::geometry::Rect;
    use crate::layout::{Color, FontWeight, TextSpan};
    TextSpan {
        provenance: None,
        text_rise: 0.0,
        artifact_type: None,
        text: text.to_string(),
        bbox: Rect::new(x, y, w, 10.0),
        font_size: 10.0,
        font_name: "Test".to_string(),
        font_weight: FontWeight::Normal,
        is_italic: false,
        is_monospace: false,
        color: Color { r: 0.0, g: 0.0, b: 0.0 },
        mcid: None,
        mcid_scope: None,
        sequence: 0,
        split_boundary_before: false,
        offset_semantic: false,
        char_spacing: 0.0,
        word_spacing: 0.0,
        horizontal_scaling: 100.0,
        primary_detected: false,
        char_widths: vec![],
        char_x_offsets: Vec::new(),
        heading_level: None,
        rotation_degrees: 0.0,
        wmode: 0,
        rtl_draw_logical: false,
        mirrored: false,
        page_rotation_applied: 0,
    }
}

/// A shared-baseline two-column prose body (academic references): each line
/// has scattered word-granular left edges in BOTH columns — so the
/// dominant-cluster-fraction gate misses it — but a single persistent
/// central gutter. The corridor accept path must route it as multi-column.
#[test]
fn test_corridor_accepts_scattered_two_column_prose() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem", 50.0, y, 35.0));
        spans.push(corridor_span("ipsumdolor", 95.0, y, 40.0));
        spans.push(corridor_span("sitametco", 140.0, y, 40.0));
        spans.push(corridor_span("consectetur", 300.0, y, 40.0));
        spans.push(corridor_span("adipiscing", 345.0, y, 40.0));
        spans.push(corridor_span("elitsedo", 390.0, y, 40.0));
    }
    assert!(
        PdfDocument::is_multi_column_page(&spans),
        "scattered-edge two-column prose with a persistent central gutter \
             must be detected as multi-column via the corridor accept path"
    );
}

/// A short-cell numeric table shares one column gap but has tiny cells
/// (mean chars per line well below 20). The prose guard must reject it so
/// the table is NOT routed to XY-cut (which would reorder its cells).
#[test]
fn test_corridor_rejects_short_cell_table() {
    let mut spans = Vec::new();
    // Scattered left edges (so the bimodal-line-start detector does NOT
    // fire and the dominant-cluster gate fails — i.e. control reaches the
    // corridor path), but every cell is a short numeric token so the
    // per-line mean char count stays well under the prose floor of 20. ~keep
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("12", 50.0, y, 12.0));
        spans.push(corridor_span("34", 95.0, y, 12.0));
        spans.push(corridor_span("56", 140.0, y, 12.0));
        spans.push(corridor_span("78", 300.0, y, 12.0));
        spans.push(corridor_span("90", 345.0, y, 12.0));
        spans.push(corridor_span("12", 390.0, y, 12.0));
    }
    assert!(
        !PdfDocument::is_multi_column_page(&spans),
        "short-cell numeric table must NOT be routed as multi-column \
             (grid-row discriminator rejects ≥2-gap rows)"
    );
}

/// Part 1a: a SHORT-line two-column verse body (Bible / lexicon) — one
/// short fragment per column, one central gutter per line — used to be
/// rejected by the raw `mean_chars <= 20` floor. It must now be admitted via
/// the corridor's short-line path (single gap/line, balanced, central).
#[test]
fn test_corridor_accepts_short_verse_two_column() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Bereshit", 50.0, y, 45.0));
        spans.push(corridor_span("barahem", 300.0, y, 40.0));
    }
    // Call the corridor directly (bypass the upstream bimodal/histogram
    // gates) with a no-op degenerate-CTM filter. ~keep
    assert!(
        PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "short-verse two-column body (1 gutter/line, balanced) must be admitted"
    );
}

/// Part 1a guard: a lopsided narrow-label + wide-data table must stay
/// rejected even though it has one gap per line — its gutter sits off-centre
/// (failing the centre gate) and its columns are lopsided (failing the
/// char-mass balance), either of which is sufficient.
#[test]
fn test_corridor_rejects_label_column_table() {
    let mut spans = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("1", 50.0, y, 8.0));
        spans.push(corridor_span("Descriptionlongdata", 300.0, y, 200.0));
    }
    assert!(
        !PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "lopsided narrow-label + wide-data table must be rejected (char balance)"
    );
}

/// Part 1b: a two-column prose body interleaved with a MINORITY of
/// full-width display-math / heading rows must still be detected — the
/// full-width rows are excluded from the coverage denominator. Without the
/// exclusion the coverage floor (best_size*2 >= lines) fails.
#[test]
fn test_corridor_survives_minority_display_math() {
    let mut spans = Vec::new();
    for i in 0..16 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor", 50.0, y, 120.0));
        spans.push(corridor_span("sit amet consectetur", 300.0, y, 150.0));
    }
    for i in 0..24 {
        let y = 400.0 - i as f32 * 14.0;
        spans.push(corridor_span("Section heading spanning width", 50.0, y, 400.0));
    }
    assert!(
        PdfDocument::has_persistent_gutter_corridor(&spans, 300.0, 10_000.0),
        "two-column prose with a minority of full-width display rows must hold"
    );
}

#[test]
fn measure_gutter_accepts_centered_two_columns() {
    // Left col →170, right col 300→450: a wide central corridor at ~235,
    // which is 0.46 of the [50,450] content width (inside 0.30..=0.70). ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor", 50.0, y, 120.0));
        spans.push(corridor_span("sit amet consectetur", 300.0, y, 150.0));
    }
    let g = PdfDocument::measure_single_central_gutter(&spans);
    assert!(g.is_some(), "centered two columns must yield a gutter");
    assert!((g.unwrap() - 235.0).abs() < 5.0, "gutter mid-x ≈ 235, got {g:?}");
}

#[test]
fn measure_gutter_rejects_single_column() {
    // One full-width column → no corridor → None (byte-identical caller). ~keep
    let mut spans = Vec::new();
    for i in 0..10 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Full width single column line", 50.0, y, 400.0));
    }
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn measure_gutter_rejects_three_column_grid() {
    // Three columns ⇒ two corridors ⇒ not a single central gutter ⇒ None
    // (grids/tables stay on their existing row-aware/structural path). ~keep
    let mut spans = Vec::new();
    for i in 0..6 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("colA", 50.0, y, 60.0));
        spans.push(corridor_span("colB", 140.0, y, 60.0));
        spans.push(corridor_span("colC", 230.0, y, 60.0));
    }
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_finds_tight_gutter_under_bridging_header() {
    // Dense two columns separated by a TIGHT ~12 pt gutter (left →291,
    // right 304→552), below the 18 pt cover-scan threshold, PLUS one
    // full-width header line that bridges the gutter (43→308). The 1-D cover
    // scan jumps its running max past the corridor and misses it; the 2-D
    // projection only counts spans that actually straddle a given x, so the
    // lone header is absorbed by the tolerance and the gutter at ~297 is
    // found. This is the PMC8129076 defect-2 case. ~keep
    let mut spans = Vec::new();
    spans.push(corridor_span("NATURE COMMUNICATIONS header line", 43.0, 760.0, 265.0));
    for i in 0..16 {
        let y = 740.0 - i as f32 * 12.0;
        spans.push(corridor_span("left column body text here ok", 43.0, y, 248.0));
        spans.push(corridor_span("right column body text here ok", 304.0, y, 248.0));
    }
    let g = PdfDocument::density_central_gutter(&spans);
    assert!(g.is_some(), "tight gutter under a bridging header must be found");
    assert!((g.unwrap() - 297.5).abs() < 8.0, "gutter mid-x ≈ 297, got {g:?}");
    // The conservative cover scan misses this (gutter < 18 pt and a header
    // bridges it), confirming the density probe adds genuinely new coverage. ~keep
    assert!(PdfDocument::measure_single_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_single_column() {
    let mut spans = Vec::new();
    for i in 0..16 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("Full width single column line of text", 50.0, y, 400.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_three_column_grid() {
    let mut spans = Vec::new();
    for i in 0..12 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("colA", 50.0, y, 60.0));
        spans.push(corridor_span("colB", 140.0, y, 60.0));
        spans.push(corridor_span("colC", 230.0, y, 60.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn density_gutter_rejects_degenerate_ctm_content_width() {
    // Two "columns" separated by a 200,000pt gap — the signature of a
    // degenerate CTM scale factor inflating span x-coordinates, not a
    // real page (a normal page is at most a few thousand points wide).
    // Before the MAX_CONTENT_EXTENT bound, the huge empty middle region
    // was itself picked up as a single "corridor" and returned as a
    // (nonsensical) gutter position; it must now be rejected outright. ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("left col text here", 50.0, y, 60.0));
        spans.push(corridor_span("right col text here", 200_050.0, y, 60.0));
    }
    assert!(PdfDocument::density_central_gutter(&spans).is_none());
}

#[test]
fn classifier_gutter_rejects_degenerate_ctm_content_width() {
    // Same degenerate-CTM hazard as the density-probe test above, for
    // `classifier_column_gutter`'s independent content_w computation. ~keep
    let mut spans = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f32 * 12.0;
        spans.push(corridor_span("left col text here", 50.0, y, 60.0));
        spans.push(corridor_span("right col text here", 200_050.0, y, 60.0));
    }
    assert!(PdfDocument::classifier_column_gutter(&spans).is_none());
}

#[test]
fn block_char_density_separates_dense_from_sparse() {
    // 5 lines of prose (~21 chars/line) is DENSE; 5 lines of bare numbers
    // (~2 chars/line) is SPARSE. med_h = 10 (corridor_span height). ~keep
    let dense: Vec<_> = (0..5)
        .map(|i| corridor_span("twenty chars of text!", 50.0, 700.0 - i as f32 * 14.0, 120.0))
        .collect();
    let sparse: Vec<_> = (0..5)
        .map(|i| corridor_span("12", 50.0, 700.0 - i as f32 * 14.0, 12.0))
        .collect();
    let dref: Vec<&_> = dense.iter().collect();
    let sref: Vec<&_> = sparse.iter().collect();
    let dd = PdfDocument::block_char_density(&dref, 10.0);
    let sd = PdfDocument::block_char_density(&sref, 10.0);
    assert!(dd > 15.0, "dense density should be high, got {dd}");
    assert!(sd < 4.0, "sparse density should be low, got {sd}");
    assert!(dd > sd * 4.0, "dense must clearly exceed sparse");
}

#[test]
fn lift_marginalia_column_lifts_left_line_numbers() {
    let mut spans = Vec::new();
    for i in 0..14 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Body prose line of real text here", 100.0, y, 300.0));
        spans.push(corridor_span(&format!("{}", 118 + i), 50.0, y, 15.0));
    }
    let lifted = PdfDocument::lift_marginalia_column(&spans).expect("rail must be lifted");
    assert_eq!(lifted.len(), 14, "all 14 line numbers lifted");
    for &i in &lifted {
        assert!(
            spans[i].text.trim().chars().all(|c| c.is_ascii_digit()),
            "lifted span must be a numeral: {:?}",
            spans[i].text
        );
    }
}

#[test]
fn lift_marginalia_column_skips_dense_first_column() {
    // Genuine two-column prose: both columns are wide + multi-word, so
    // neither sits inside the narrow outer strip → no lift (byte-identical). ~keep
    let mut spans = Vec::new();
    for i in 0..14 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Lorem ipsum dolor sit", 50.0, y, 120.0));
        spans.push(corridor_span("amet consectetur elit", 300.0, y, 150.0));
    }
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}

#[test]
fn lift_marginalia_column_skips_abutting_label_column() {
    // A narrow short-token left column whose gutter to the body is < 18 pt
    // (an abutting table label column, not a detached rail) → no lift. ~keep
    let mut spans = Vec::new();
    for i in 0..10 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("AB", 50.0, y, 15.0));
        spans.push(corridor_span("Body prose text here long", 70.0, y, 200.0));
    }
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}

#[test]
fn lift_marginalia_column_skips_single_page_number() {
    // A single stray numeral (1 rail line) fails the ≥3-line gate → no lift. ~keep
    let mut spans = Vec::new();
    for i in 0..12 {
        let y = 700.0 - i as f32 * 14.0;
        spans.push(corridor_span("Body prose line of real text", 100.0, y, 300.0));
    }
    spans.push(corridor_span("7", 50.0, 500.0, 12.0));
    assert!(PdfDocument::lift_marginalia_column(&spans).is_none());
}

#[test]
fn test_extract_page_text_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc.extract_page_text(0).unwrap();
    assert!(page_text.spans.is_empty());
    assert!(page_text.chars.is_empty());
    // MediaBox is [0 0 612 792] in build_minimal_pdf ~keep
    assert!((page_text.page_width - 612.0).abs() < 0.1);
    assert!((page_text.page_height - 792.0).abs() < 0.1);
}

#[test]
fn test_extract_page_text_has_page_dimensions() {
    let content = b"BT /F1 12 Tf (Hello) Tj ET";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc.extract_page_text(0).unwrap();
    assert!((page_text.page_width - 612.0).abs() < 0.1);
    assert!((page_text.page_height - 792.0).abs() < 0.1);
}

#[test]
fn test_extract_page_text_chars_derived_from_spans() {
    let content = b"BT /F1 12 Tf (Hello) Tj ET";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc.extract_page_text(0).unwrap();
    let expected_char_count: usize = page_text.spans.iter().map(|s| s.text.chars().count()).sum();
    assert_eq!(page_text.chars.len(), expected_char_count);
}

#[test]
fn test_extract_page_text_with_column_aware() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc
        .extract_page_text_with_options(0, ReadingOrder::ColumnAware)
        .unwrap();
    assert!(page_text.spans.is_empty());
    assert!((page_text.page_width - 612.0).abs() < 0.1);
}

#[test]
fn test_extract_page_text_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let result = doc.extract_page_text(99);
    assert!(result.is_err());
}

/// Regression test: Tm-scale containment filter must not
/// drop distinct text lines whose bounding boxes overlap spatially.
///
/// Before the fix, the containment filter in extract_text() would skip any
/// span geometrically contained within the previous span, even if the text
/// was different. This caused the second line to silently disappear.
///
/// The fix adds a `span.text == prev.text` guard so that only true
/// duplicates are filtered.
#[test]
fn test_containment_filter_preserves_distinct_overlapping_lines() {
    // Build a minimal PDF with two Td-placed text strings at very close Y
    // positions (Y=700 and Y=699 — within the 2.0pt "same line" threshold)
    // but with different content. The first string is wider so the second
    // is geometrically contained within it. ~keep
    let content = b"BT /F1 12 Tf 50 700 Td (First line has longer text here) Tj 0 -1 Td (Second) Tj ET";

    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        );

    let off4 = pdf.len();
    let content_len = content.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content_len).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let text = doc.extract_text(0).unwrap();

    assert!(
        text.contains("First line has longer text here"),
        "First line should be present in extracted text, got: {:?}",
        text
    );
    assert!(
        text.contains("Second"),
        "Second line must NOT be dropped by containment filter, got: {:?}",
        text
    );
}

/// `extract_spans`/`extract_words`/`extract_text_lines` must report the
/// resolved `/BaseFont` name ("Helvetica"), not the page's
/// `/Resources/Font` dictionary alias ("F1") — matching what
/// `extract_chars` already did.
#[test]
fn test_span_word_line_font_name_is_resolved_not_alias() {
    let content = b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET";

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        );
    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let chars = doc.extract_chars(0).unwrap();
    assert_eq!(chars[0].font_name, "Helvetica");

    let spans = doc.extract_spans(0).unwrap();
    assert_eq!(spans[0].font_name, "Helvetica");

    let words = doc.extract_words(0).unwrap();
    assert_eq!(words[0].dominant_font, "Helvetica");

    let lines = doc.extract_text_lines(0).unwrap();
    assert_eq!(lines[0].words[0].dominant_font, "Helvetica");
}

/// `Word.sequence` must reflect content-stream emission order (the
/// originating span's `sequence`), not just reading order, so
/// consumers can tell genuinely-consecutive draw calls apart from
/// spatially-close-but-stream-distant ones (e.g. table cells vs.
/// overlays).
#[test]
fn test_extract_words_sequence_reflects_stream_order() {
    let content = b"BT /F1 12 Tf 50 700 Td (First) Tj 0 -20 Td (Second) Tj ET";

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        );
    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let off5 = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off5).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let words = doc.extract_words(0).unwrap();
    let first = words.iter().find(|w| w.text == "First").unwrap();
    let second = words.iter().find(|w| w.text == "Second").unwrap();
    assert!(
        first.sequence < second.sequence,
        "word drawn first in the content stream must have the smaller sequence: \
             First={}, Second={}",
        first.sequence,
        second.sequence
    );

    let lines = doc.extract_text_lines(0).unwrap();
    let line_words: Vec<&crate::layout::Word> = lines.iter().flat_map(|l| l.words.iter()).collect();
    let first_line_word = line_words.iter().find(|w| w.text == "First").unwrap();
    let second_line_word = line_words.iter().find(|w| w.text == "Second").unwrap();
    assert!(
        first_line_word.sequence < second_line_word.sequence,
        "extract_text_lines words must also carry stream-order sequence"
    );
}

#[test]
fn test_page_text_serializable() {
    let page_text = crate::layout::PageText {
        spans: Vec::new(),
        chars: Vec::new(),
        page_width: 612.0,
        page_height: 792.0,
    };
    let json = serde_json::to_string(&page_text).unwrap();
    // Without the `wasm` feature, field names are snake_case ~keep
    assert!(json.contains("page_width"));
    assert!(json.contains("page_height"));
}

#[test]
fn test_fix_digit_logicalnot_decimal() {
    // `¬` between digits → `.`; spaced or non-digit-flanked `¬` is left alone. ~keep
    assert_eq!(PdfDocument::fix_digit_logicalnot_decimal("1\u{00AC}00"), "1.00");
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("0\u{00AC}75 1\u{00AC}00"),
        "0.75 1.00"
    );
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("A \u{00AC} B"),
        "A \u{00AC} B"
    );
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("5 \u{00AC} 3"),
        "5 \u{00AC} 3"
    );
    assert_eq!(PdfDocument::fix_digit_logicalnot_decimal("\u{00AC}5"), "\u{00AC}5");
    // Spaced decimal: a subset that emits a single space between the decimal
    // glyph and the fractional digits → drop the lone space, recover `.`. ~keep
    assert_eq!(PdfDocument::fix_digit_logicalnot_decimal("1\u{00AC} 00"), "1.00");
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("0\u{00AC} 75 1\u{00AC} 00"),
        "0.75 1.00"
    );
    // Still NOT a decimal when the leading digit does not abut `¬`
    // (genuine spaced negation): `5 ¬ 3` stays untouched even though a
    // digit follows the space. ~keep
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("5 \u{00AC} 3"),
        "5 \u{00AC} 3"
    );
    assert_eq!(
        PdfDocument::fix_digit_logicalnot_decimal("1\u{00AC}  00"),
        "1\u{00AC}  00"
    );
}

#[test]
fn test_is_cm_or_symbol_font() {
    assert!(PdfDocument::is_cm_or_symbol_font("ABCDEF+CMSY10"));
    assert!(PdfDocument::is_cm_or_symbol_font("CMR12"));
    assert!(PdfDocument::is_cm_or_symbol_font("Symbol"));
    assert!(!PdfDocument::is_cm_or_symbol_font("ABCDEF+Helvetica"));
    assert!(!PdfDocument::is_cm_or_symbol_font("TimesNewRoman"));
}

/// A password-protected PDF is detected as encrypted, and text extraction
/// degrades to empty output (warn + empty) rather than erroring — matching
/// pdftotext/PyMuPDF. (`page_count` still surfaces `Error::EncryptedPdf`;
/// see `tests/test_extraction_robustness.rs`.)
#[test]
fn test_encrypted_pdf_extracts_empty_without_password() {
    let pdf_path = "tests/fixtures/encrypted_needs_password.pdf";
    let doc = PdfDocument::open(pdf_path).expect("open should succeed even without password");
    assert!(doc.is_encrypted(), "PDF should be detected as encrypted");

    let text = doc
        .extract_text(0)
        .expect("extract_text degrades to empty, not an error");
    assert!(
        text.is_empty(),
        "undecryptable extraction should be empty, got: {:?}",
        text,
    );
}

/// After authenticating with the correct password, extraction should succeed.
#[test]
fn test_encrypted_pdf_works_after_authentication() {
    let pdf_path = "tests/fixtures/encrypted_needs_password.pdf";
    let doc = PdfDocument::open(pdf_path).expect("open should succeed");
    assert!(doc.is_encrypted());

    let result = doc.authenticate(b"secret").expect("authenticate should not error");
    assert!(result, "Authentication with correct password should succeed");

    let page_count = doc.page_count().expect("page_count should work after auth");
    assert!(page_count > 0, "Should have at least 1 page after auth");

    // extract_text should not error (content may be minimal since it's a test PDF) ~keep
    let _text = doc.extract_text(0).expect("extract_text should work after auth");
}

/// Multi-row-spanning label cell (test item name vertically centered
/// across N data rows) must be placed at the top of its row block in
/// reading-order output, not interleaved mid-group by Y.
///
/// Simulates a simplified 2-column table:
/// - Column A (sparse, "labels"): 2 labels, each centered in its
///   block of 6 data rows.
/// - Column B (dense, "data"): 12 data rows.
///
/// Expected sort: Label1, d1..d6, Label2, d7..d12.
#[test]
fn test_rowspan_label_promoted_to_top_of_block() {
    use crate::layout::TextSpan;

    fn mk(text: &str, x: f32, y: f32, w: f32) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: crate::geometry::Rect::new(x, y, w, 10.0),
            font_size: 12.0,
            font_name: "Arial".into(),
            font_weight: crate::layout::FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: crate::layout::Color::black(),
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    // Data rows at x=200, y=100..30 step -10 (12 rows).
    // Label1 at x=50, y=75 (middle of rows 100..60).
    // Label2 at x=50, y=45 (middle of rows 50..30... but actually 50..30 is 3 values,
    //   and label2 should be centered in rows 50..30 → y=40 but we choose 45 to be clearly in 2nd block).
    // Target split: Label1 owns rows 100,90,80,70,60,50; Label2 owns 40,30,20,10.
    // Both labels' Y (75 and 45) sit between their block rows. ~keep
    let mut spans = vec![mk("L1", 50.0, 75.0, 40.0), mk("L2", 50.0, 45.0, 40.0)];
    for i in 0..12 {
        let y = 100.0 - (i as f32) * 10.0;
        spans.push(mk(&format!("d{:02}", i), 200.0, y, 20.0));
    }

    super::PdfDocument::reorder_rowspan_labels(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    let pos_l1 = texts.iter().position(|t| *t == "L1").expect("L1 present");
    let pos_l2 = texts.iter().position(|t| *t == "L2").expect("L2 present");
    assert!(
        pos_l1 < pos_l2,
        "L1 should precede L2 in reading order, got {:?}",
        texts
    );
    // L1 must come before ALL data rows that belong to L1's block.
    // With distance-based partitioning, L1 owns rows closer to y=75 than y=45:
    //   100,90,80,70,60 are closer to 75. 50 is equidistant (tie → L1).
    //   Expect L1 at index 0 and L2 somewhere after L1's block. ~keep
    assert_eq!(texts[0], "L1", "L1 must be first, got: {:?}", &texts[..5]);
    assert!(
        pos_l2 > pos_l1 + 3,
        "L2 must come after several data rows of L1's block, got {:?}",
        texts
    );
}

/// Regression: line-continuation spans that share a Y-band with the dense
/// column must NOT be promoted by `reorder_rowspan_labels`.
///
/// A resume-like PDF has two X groups: a dense main-text column (x=63)
/// and a sparse rightward column (x=430) whose spans are all on the SAME
/// lines as the dense column (same Y-bands). The sparse spans are
/// line-continuation text, not rowspan labels, so they must stay in their
/// natural sorted position rather than being hoisted to wrong Y values.
#[test]
fn test_rowspan_label_skips_spans_aligned_with_dense_column() {
    use crate::layout::TextSpan;

    fn mk(text: &str, x: f32, y: f32) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: crate::geometry::Rect::new(x, y, 80.0, 10.0),
            font_size: 12.0,
            font_name: "Arial".into(),
            font_weight: crate::layout::FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: crate::layout::Color::black(),
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    // Dense column (x=63): 10 spans at y=640,620,600,580,560,540,520,500,480,460
    // Sparse column (x=430): 4 spans at y=600,560,520,480 — same lines as dense
    // After reorder_rowspan_labels the sparse spans must NOT be promoted. ~keep
    let ys_dense = [640.0f32, 620.0, 600.0, 580.0, 560.0, 540.0, 520.0, 500.0, 480.0, 460.0];
    let ys_sparse = [600.0f32, 560.0, 520.0, 480.0];

    let mut spans: Vec<TextSpan> = Vec::new();
    for &y in &ys_dense {
        spans.push(mk(&format!("dense_y{}", y as i32), 63.0, y));
    }
    for &y in &ys_sparse {
        spans.push(mk(&format!("sparse_y{}", y as i32), 430.0, y));
    }

    // Sort descending Y, X ascending (as extract_spans does before calling this) ~keep
    spans.sort_by(|a, b| crate::utils::row_aware_span_cmp(a.bbox.y, a.bbox.x, b.bbox.y, b.bbox.x));
    let before: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();

    super::PdfDocument::reorder_rowspan_labels(&mut spans);

    let after: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    assert_eq!(
        before, after,
        "reorder_rowspan_labels must not change order when sparse spans \
             share Y-bands with the dense column; \
             before={before:?} after={after:?}"
    );
}

/// Regression: a numbered reference/bibliography list whose markers
/// ("1.", "2.", …) sit in a narrow left column between body rows must
/// NOT have those markers promoted as rowspan labels. The geometry is
/// identical to a genuine rowspan table — only the marker TEXT (a
/// vertical numbered list) distinguishes it — so the guard keys on the
/// numbered-marker signal and leaves reading order intact.
#[test]
fn test_rowspan_skips_numbered_reference_continuation() {
    use crate::layout::TextSpan;

    fn mk(text: &str, x: f32, y: f32, w: f32) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: crate::geometry::Rect::new(x, y, w, 10.0),
            font_size: 12.0,
            font_name: "Arial".into(),
            font_weight: crate::layout::FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: crate::layout::Color::black(),
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    // Dense body column (x=200): 12 rows y=100..-10 step -10.
    // Numbered markers (x=50): "1.".."4." sitting BETWEEN body rows —
    // the exact geometry that promotes a genuine rowspan label. ~keep
    let mut spans = vec![
        mk("1.", 50.0, 95.0, 40.0),
        mk("2.", 50.0, 65.0, 40.0),
        mk("3.", 50.0, 35.0, 40.0),
        mk("4.", 50.0, 5.0, 40.0),
    ];
    for i in 0..12 {
        let y = 100.0 - (i as f32) * 10.0;
        spans.push(mk(&format!("b{:02}", i), 200.0, y, 20.0));
    }

    // Sort as extract_spans does before calling reorder_rowspan_labels. ~keep
    spans.sort_by(|a, b| crate::utils::row_aware_span_cmp(a.bbox.y, a.bbox.x, b.bbox.y, b.bbox.x));
    let before: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();

    super::PdfDocument::reorder_rowspan_labels(&mut spans);

    let after: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    assert_eq!(
        before, after,
        "numbered reference markers must not be promoted as rowspan \
             labels; before={before:?} after={after:?}"
    );
}

/// PDFs that are encrypted but authenticated with empty password (the common
/// case for permission-only encryption) must continue to work without error.
#[test]
fn test_encrypted_pdf_with_empty_password_still_works() {
    let pdf_path = "tests/fixtures/encrypted_cid_truetype.pdf";
    let doc = PdfDocument::open(pdf_path).expect("open should succeed");
    // This PDF auto-authenticates with empty password during open() ~keep
    assert!(doc.is_encrypted(), "Should be detected as encrypted");

    let page_count = doc.page_count().expect("page_count should work");
    assert!(page_count > 0);

    let text = doc.extract_text(0).expect("extract_text should work");
    assert!(!text.trim().is_empty(), "Should extract non-empty text");
}

#[test]
fn test_encrypted_pdf_with_compressed_object_streams() {
    // Encrypted PDFs with /Type /ObjStm streams must NOT have those streams
    // decrypted, per ISO 32000-1 Section 7.6.2. Object streams and XRef
    // streams are never individually encrypted; only the overall stream
    // data is compressed. Attempting to decrypt them causes AES errors
    // because the data length is not a multiple of the block size. ~keep
    let pdf_path = "tests/fixtures/encrypted_objstm.pdf";
    let doc = PdfDocument::open(pdf_path).expect("open should succeed for encrypted+objstm PDF");
    assert!(doc.is_encrypted(), "Should be detected as encrypted");

    let page_count = doc.page_count().expect("page_count should work with encrypted objstm");
    assert!(page_count > 0, "Should have at least one page");
}

#[test]
fn test_lock_or_recover_on_poisoned_mutex() {
    use std::sync::Mutex;
    let m = Mutex::new(42);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = m.lock().unwrap();
        panic!("intentional");
    }));
    assert!(m.lock().is_err(), "Mutex should be poisoned");
    let val = *m.lock_or_recover();
    assert_eq!(val, 42);
}

#[test]
fn test_bounded_entry_cache_lru_eviction_order() {
    let mut c = BoundedEntryCache::new(3);
    c.insert(1u32, "a");
    c.insert(2, "b");
    c.insert(3, "c");
    assert_eq!(c.get(&1), Some(&"a"));
    // Insert key 4 — should evict 2 (oldest untouched), not 1 ~keep
    c.insert(4, "d");
    assert_eq!(c.get(&1), Some(&"a"), "LRU-promoted key should survive");
    assert!(c.get(&2).is_none(), "Oldest untouched key should be evicted");
    assert_eq!(c.get(&3), Some(&"c"));
    assert_eq!(c.get(&4), Some(&"d"));
}

#[test]
fn test_bounded_entry_cache_reinsert_no_eviction() {
    let mut c = BoundedEntryCache::new(1);
    c.insert(1u32, "a");
    // Re-insert same key — should NOT evict, just replace ~keep
    c.insert(1, "b");
    assert_eq!(c.len(), 1);
    assert_eq!(c.get(&1), Some(&"b"));
}

#[test]
fn test_bounded_entry_cache_fifo_eviction_without_get() {
    let mut c = BoundedEntryCache::new(2);
    c.insert(1u32, "a");
    c.insert(2, "b");
    // No get() calls — pure insertion order ~keep
    c.insert(3, "c");
    assert!(c.get(&1).is_none(), "First inserted should be evicted");
    assert_eq!(c.get(&2), Some(&"b"));
    assert_eq!(c.get(&3), Some(&"c"));
}

#[test]
fn test_bounded_object_cache_oversized_rejection() {
    let mut c = BoundedObjectCache::new(100);
    let big = Object::String(vec![0u8; 200]);
    c.insert(ObjectRef::new(1, 0), big);
    assert_eq!(c.len(), 0, "Oversized object should be rejected");
}

#[test]
fn test_bounded_object_cache_byte_budget_eviction() {
    // Use a budget that fits ~2 small objects but not 3 ~keep
    let small = Object::Integer(1);
    let budget = 80;
    let mut c = BoundedObjectCache::new(budget);
    c.insert(ObjectRef::new(1, 0), small.clone());
    c.insert(ObjectRef::new(2, 0), small.clone());
    assert_eq!(c.len(), 2);
    c.insert(ObjectRef::new(3, 0), small.clone());
    assert!(c.get(&ObjectRef::new(1, 0)).is_none(), "Oldest should be evicted");
    assert!(c.get(&ObjectRef::new(3, 0)).is_some());
    assert!(c.current_bytes <= budget);
}

#[test]
fn object_stream_cache_rejects_oversized_entries_and_evicts_to_budget() {
    let first = Arc::new(HashMap::from([(1, Object::String(vec![0; 64]))]));
    let second = Arc::new(HashMap::from([(2, Object::String(vec![0; 64]))]));
    let third = Arc::new(HashMap::from([(3, Object::String(vec![0; 64]))]));
    let entry_bytes = BoundedObjectStreamCache::estimate_size(&first).unwrap();
    let budget = entry_bytes * 2;
    let mut cache = BoundedObjectStreamCache::new(budget);

    assert!(cache.insert(ObjectRef::new(10, 0), Arc::clone(&first)));
    assert!(cache.insert(ObjectRef::new(11, 0), Arc::clone(&second)));
    assert!(cache.insert(ObjectRef::new(12, 0), Arc::clone(&third)));
    assert!(
        cache.get(&ObjectRef::new(10, 0)).is_none(),
        "oldest stream must be evicted"
    );
    assert!(cache.get(&ObjectRef::new(12, 0)).is_some());
    assert!(cache.current_bytes <= budget);

    let oversized = Arc::new(HashMap::from([(4, Object::String(vec![0; budget]))]));
    assert!(!cache.insert(ObjectRef::new(13, 0), oversized));
    assert!(cache.get(&ObjectRef::new(13, 0)).is_none());
    assert!(cache.current_bytes <= budget);
}

#[test]
fn object_stream_cache_rejects_deeply_nested_oversized_string() {
    let mut nested = Object::String(vec![0; 1024 * 1024]);
    for _ in 0..9 {
        nested = Object::Array(vec![nested]);
    }
    let mut cache = BoundedObjectStreamCache::new(4 * 1024);

    assert!(!cache.insert(ObjectRef::new(10, 0), Arc::new(HashMap::from([(20, nested)]))));
    assert_eq!(cache.map.len(), 0);
    assert_eq!(cache.current_bytes, 0);
}

#[test]
fn object_stream_cache_accounts_for_nested_stream_bytes() {
    let nested = Object::Array(vec![Object::Dictionary(HashMap::from([(
        "payload".to_string(),
        Object::Stream {
            dict: HashMap::new(),
            data: bytes::Bytes::from(vec![0; 1024 * 1024]),
        },
    )]))]);
    let mut cache = BoundedObjectStreamCache::new(4 * 1024);

    assert!(!cache.insert(ObjectRef::new(10, 0), Arc::new(HashMap::from([(20, nested)]))));
    assert_eq!(cache.map.len(), 0);
    assert_eq!(cache.current_bytes, 0);
}

#[test]
fn object_stream_cache_rejects_accounting_overflow() {
    assert_eq!(BoundedObjectStreamCache::checked_capacity_bytes(usize::MAX, 2), None);
}

#[test]
fn object_stream_recovery_marker_is_bounded_and_fail_closed() {
    let mut marker = BoundedRecoveryTelemetry::new(1);

    assert!(marker.should_emit(10));
    assert!(!marker.should_emit(10));
    assert!(!marker.should_emit(11));
    assert!(marker.saturated);
    assert_eq!(marker.seen.len(), 0);
    assert!(!marker.should_emit(10));
}

#[test]
fn test_estimate_size_depth_bottoms_out() {
    // Deeply nested array — should not stack overflow ~keep
    let mut obj = Object::Integer(1);
    for _ in 0..100 {
        obj = Object::Array(vec![obj]);
    }
    // Should return a finite value without panicking ~keep
    let size = BoundedObjectCache::estimate_size(&obj);
    assert!(size > 0);
}

// -----------------------------------------------------------------
// PdfDocument::contains_rect_with_tolerance
//
// Pins the table-retain tolerance behaviour: spans whose f32
// right-edge drifts a fraction of a point past the table bbox
// (due to accumulated width-sum error) must still count as
// contained, but spans that actually extend beyond the table
// must not. Each test's first block is a geometry sanity check
// so a Rect::new construction mistake fails loudly rather than
// silently exercising the wrong geometry.
// ----------------------------------------------------------------- ~keep

#[test]
fn contains_rect_with_tolerance_absorbs_subpixel_drift() {
    use crate::geometry::Rect;
    let table = Rect::new(0.0, 0.0, 100.0, 100.0);
    let drifted = Rect::new(10.0, 10.0, 90.02, 80.0);

    // Geometry sanity: drifted span right-edge should sit ~0.02pt
    // past table right-edge. If this fails, the test construction
    // is wrong, not the tolerance logic. Tolerance is 1e-4pt
    // because `0.02f32` is not representable exactly — the
    // observed drift lands within ~4e-6 of 0.02. ~keep
    assert!(
        (drifted.right() - table.right() - 0.02).abs() < 1e-4,
        "drifted span right-edge should be 0.02pt past table right-edge; got drift = {}",
        drifted.right() - table.right()
    );
    assert_eq!(drifted.left(), 10.0, "span should start at x=10");
    assert_eq!(drifted.top(), 10.0, "span should start at y=10");
    assert_eq!(drifted.bottom(), 90.0, "span should end at y=90");

    assert!(PdfDocument::contains_rect_with_tolerance(&table, &drifted, 0.1));
}

#[test]
fn contains_rect_with_tolerance_rejects_genuinely_outside() {
    use crate::geometry::Rect;
    let table = Rect::new(0.0, 0.0, 100.0, 100.0);
    let outside = Rect::new(10.0, 10.0, 91.0, 80.0);

    assert!(
        (outside.right() - table.right() - 1.0).abs() < 1e-6,
        "outside span right-edge should be 1.0pt past table right-edge; got drift = {}",
        outside.right() - table.right()
    );

    assert!(!PdfDocument::contains_rect_with_tolerance(&table, &outside, 0.1));
}

#[test]
fn contains_rect_with_tolerance_accepts_fully_inside() {
    use crate::geometry::Rect;
    let table = Rect::new(0.0, 0.0, 100.0, 100.0);
    let inside = Rect::new(10.0, 10.0, 80.0, 80.0);

    assert!(
        inside.left() > table.left()
            && inside.right() < table.right()
            && inside.top() > table.top()
            && inside.bottom() < table.bottom(),
        "control span should be strictly inside the table"
    );

    assert!(PdfDocument::contains_rect_with_tolerance(&table, &inside, 0.1));
}

/// Regression test (pdfa_036): span filtering must use per-cell
/// bboxes, not the coarser outer table bbox.
///
/// Before the fix, `span_in_table` filtered by `table.bbox`, which could
/// be wider than the union of the actual cell bboxes. Paragraph text that
/// happened to fall inside the table's outer bbox was silently dropped even
/// though no cell claimed it, causing content loss (the "(HLA)/(KSL)"
/// paragraph in pdfa_036 disappeared).
///
/// After the fix, only spans inside at least one *cell* bbox are removed
/// from the flow. Spans inside the outer table bbox but outside all cells
/// (i.e. in a gap or margin) are preserved.
#[test]
fn cell_bbox_filter_preserves_span_in_outer_bbox_gap() {
    use crate::geometry::Rect;
    use crate::structure::table_extractor::{Table, TableCell, TableRow};

    // A table whose outer bbox is [0, 0] – [200, 100].
    // Two non-adjacent cells leave a horizontal gap at x=90..110 — that
    // gap is inside the outer bbox but not inside any cell. ~keep
    let mut table = Table::new();
    let mut row = TableRow::new(false);
    row.cells.push(TableCell {
        text: "left".to_string(),
        spans: vec![],
        colspan: 1,
        rowspan: 1,
        mcids: vec![],
        bbox: Some(Rect::new(0.0, 0.0, 90.0, 100.0)),
        is_header: false,
    });
    row.cells.push(TableCell {
        text: "right".to_string(),
        spans: vec![],
        colspan: 1,
        rowspan: 1,
        mcids: vec![],
        bbox: Some(Rect::new(110.0, 0.0, 90.0, 100.0)),
        is_header: false,
    });
    table.add_row(row);
    table.bbox = Some(Rect::new(0.0, 0.0, 200.0, 100.0));

    const TOL: f32 = 0.1;

    let span_cell = Rect::new(10.0, 10.0, 50.0, 20.0);
    let in_any_cell = table.rows.iter().any(|r| {
        r.cells.iter().any(|c| {
            c.bbox
                .is_some_and(|b| PdfDocument::contains_rect_with_tolerance(&b, &span_cell, TOL))
        })
    });
    assert!(in_any_cell, "span inside a cell bbox must be identified as in-table");

    let span_gap = Rect::new(95.0, 10.0, 10.0, 20.0);

    // 1. Outer-bbox filter (the OLD, incorrect approach) would classify it as in-table. ~keep
    let in_outer_bbox = PdfDocument::contains_rect_with_tolerance(&table.bbox.unwrap(), &span_gap, TOL);
    assert!(
        in_outer_bbox,
        "gap span must be inside the outer table bbox (precondition for the bug to trigger)"
    );

    // 2. Cell-bbox filter (the NEW, correct approach) must NOT classify it as in-table. ~keep
    let in_any_cell_gap = table.rows.iter().any(|r| {
        r.cells.iter().any(|c| {
            c.bbox
                .is_some_and(|b| PdfDocument::contains_rect_with_tolerance(&b, &span_gap, TOL))
        })
    });
    assert!(
        !in_any_cell_gap,
        "gap span must NOT be inside any cell bbox — cell-bbox filter must preserve it"
    );
}

#[test]
fn reorder_same_line_runs_preserves_disjoint_x_rows() {
    use crate::geometry::Rect;
    use crate::layout::TextSpan;

    // Two rows close enough in Y to pass the existing same_line_threshold:
    // Δy = 4.5 and fs = 10, so threshold = 5.0.
    // They are disjoint in X (gap of 225pt = 22.5 * fs, well over the
    // SAME_LINE_REORDER_MAX_GAP_FACTOR = 3.0 ceiling). The helper must
    // not X-sort them into [skersey, VerDate]; it must preserve the
    // row-aware order. ~keep
    let mut spans = vec![
        TextSpan {
            text: "VerDate".to_string(),
            bbox: Rect::new(350.0, 200.0, 85.0, 10.0),
            font_size: 10.0,
            sequence: 0,
            ..Default::default()
        },
        TextSpan {
            text: "skersey".to_string(),
            bbox: Rect::new(50.0, 195.5, 75.0, 10.0),
            font_size: 10.0,
            sequence: 1,
            ..Default::default()
        },
    ];

    PdfDocument::reorder_same_line_runs(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["VerDate", "skersey"]);
}

#[test]
fn reorder_same_line_runs_orders_suffix_superscript_by_x() {
    use crate::geometry::Rect;
    use crate::layout::TextSpan;

    // Row-aware/Y-desc order can put the superscript first because it
    // sits higher. The tentative X-gap validation must not reject this
    // legitimate mixed-baseline run; the X-sorted gaps are 15pt and 0pt
    // at max_fs=14, both well under 3.0 * 14 = 42. Final order should
    // be normal left-to-right text. ~keep
    let mut spans = vec![
        TextSpan {
            text: "th".to_string(),
            bbox: Rect::new(180.0, 205.0, 10.0, 10.0),
            font_size: 10.0,
            sequence: 0,
            ..Default::default()
        },
        TextSpan {
            text: "September".to_string(),
            bbox: Rect::new(100.0, 200.0, 50.0, 14.0),
            font_size: 14.0,
            sequence: 1,
            ..Default::default()
        },
        TextSpan {
            text: "11".to_string(),
            bbox: Rect::new(165.0, 200.0, 15.0, 14.0),
            font_size: 14.0,
            sequence: 2,
            ..Default::default()
        },
    ];

    PdfDocument::reorder_same_line_runs(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["September", "11", "th"]);
}

#[test]
fn reorder_same_line_runs_de_interleaves_two_stacked_lines() {
    use crate::geometry::Rect;
    use crate::layout::TextSpan;

    // Two lines the same-line tolerance merged into one band: at fs=10 the
    // threshold is max(10*1.2, 10*0.3)=12, and the lines are 8pt apart — so
    // they group as one run, yet 8 > 0.5*fs (=5) makes them TWO stacked rows
    // of two spans each. Their X-extents overlap, so a flat X-sort would
    // interleave them word-by-word ("The Story Book Review"). The
    // de-interleave path must instead order (Y-desc, then X) so each real
    // line stays contiguous: line one ("The Book") then line two
    // ("Story Review"). Input is given in the interleaved X order the
    // row-aware sort would produce. ~keep
    let span = |t: &str, x: f32, y: f32, w: f32, seq: usize| TextSpan {
        text: t.to_string(),
        bbox: Rect::new(x, y, w, 10.0),
        font_size: 10.0,
        sequence: seq,
        ..Default::default()
    };
    let mut spans = vec![
        span("The", 100.0, 200.0, 30.0, 0),
        span("Story", 110.0, 192.0, 40.0, 1),
        span("Book", 140.0, 200.0, 40.0, 2),
        span("Review", 150.0, 192.0, 55.0, 3),
    ];

    PdfDocument::reorder_same_line_runs(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["The", "Book", "Story", "Review"],
        "stacked lines must de-interleave, not X-sort into one fake line"
    );
}

fn oc_test_doc() -> PdfDocument {
    PdfDocument::from_bytes(build_minimal_pdf(b"")).unwrap()
}

fn ocg_dict(name: Object) -> Object {
    let mut d = std::collections::HashMap::new();
    d.insert("Type".to_string(), Object::Name("OCG".to_string()));
    d.insert("Name".to_string(), name);
    Object::Dictionary(d)
}

fn ocmd_dict(ocgs: Object) -> Object {
    let mut d = std::collections::HashMap::new();
    d.insert("Type".to_string(), Object::Name("OCMD".to_string()));
    d.insert("OCGs".to_string(), ocgs);
    Object::Dictionary(d)
}

fn utf16_string(s: &str, big_endian: bool) -> Object {
    let mut bytes = if big_endian { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
    for u in s.encode_utf16() {
        if big_endian {
            bytes.extend_from_slice(&u.to_be_bytes());
        } else {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
    }
    Object::String(bytes)
}

#[test]
fn test_oc_name_ocg_ascii() {
    let doc = oc_test_doc();
    let dict = ocg_dict(Object::String(b"A-GRID".to_vec()));
    assert_eq!(doc.read_oc_name(dict.as_dict().unwrap(), 8).as_deref(), Some("A-GRID"));
}

#[test]
fn test_oc_name_ocg_utf16le_bom() {
    // Regression for the reuse of decode_pdf_text_string: the previous
    // inline reader only handled UTF-16BE and fell back to latin-1,
    // mangling UTF-16LE-encoded layer names. The shared helper decodes
    // the LE BOM correctly. ~keep
    let doc = oc_test_doc();
    let dict = ocg_dict(utf16_string("ÁREA-Ø", false));
    assert_eq!(doc.read_oc_name(dict.as_dict().unwrap(), 8).as_deref(), Some("ÁREA-Ø"));
}

#[test]
fn test_oc_name_ocg_utf16be_bom() {
    let doc = oc_test_doc();
    let dict = ocg_dict(utf16_string("EJES", true));
    assert_eq!(doc.read_oc_name(dict.as_dict().unwrap(), 8).as_deref(), Some("EJES"));
}

#[test]
fn test_oc_name_ocmd_single_ocg() {
    // OCMD has no /Name — resolution follows /OCGs (single OCG) to its name. ~keep
    let doc = oc_test_doc();
    let ocmd = ocmd_dict(ocg_dict(Object::String(b"M-DUCT".to_vec())));
    assert_eq!(doc.read_oc_name(ocmd.as_dict().unwrap(), 8).as_deref(), Some("M-DUCT"));
}

#[test]
fn test_oc_name_ocmd_ocgs_array_first_wins() {
    // /OCGs may be an array of OCGs; the first resolvable member wins. ~keep
    let doc = oc_test_doc();
    let arr = Object::Array(vec![
        ocg_dict(Object::String(b"S-COLS".to_vec())),
        ocg_dict(Object::String(b"S-BEAM".to_vec())),
    ]);
    let ocmd = ocmd_dict(arr);
    assert_eq!(doc.read_oc_name(ocmd.as_dict().unwrap(), 8).as_deref(), Some("S-COLS"));
}

#[test]
fn test_oc_name_ocmd_depth_guard() {
    // A pathological OCMD chain (each /OCGs points to another OCMD) must
    // terminate via the depth guard rather than recursing without bound. ~keep
    let doc = oc_test_doc();
    let mut nested = ocmd_dict(Object::Array(vec![]));
    for _ in 0..20 {
        nested = ocmd_dict(nested);
    }
    assert_eq!(doc.read_oc_name(nested.as_dict().unwrap(), 8), None);
}

#[test]
fn test_resolve_oc_name_via_resources_properties() {
    // Case 2 (name reference) resolves against the *passed-in* resources.
    // This is the crux of the Form-XObject fix: the resolver reads
    // /Properties /<name> from whatever resource scope the caller hands
    // it — page /Resources at page level, the XObject's own /Resources
    // when extracting inside a Form XObject. ~keep
    let doc = oc_test_doc();
    let mut props = std::collections::HashMap::new();
    props.insert("MC0".to_string(), ocg_dict(Object::String(b"A-WALL-DIM".to_vec())));
    let mut resources = std::collections::HashMap::new();
    resources.insert("Properties".to_string(), Object::Dictionary(props));
    let resources = Object::Dictionary(resources);

    let name_ref = Object::Name("MC0".to_string());
    assert_eq!(
        doc.resolve_oc_layer_name(Some(&resources), &name_ref).as_deref(),
        Some("A-WALL-DIM")
    );
}

#[test]
fn test_resolve_oc_name_inline_dict() {
    let doc = oc_test_doc();
    let inline = ocg_dict(Object::String(b"CORTES".to_vec()));
    assert_eq!(doc.resolve_oc_layer_name(None, &inline).as_deref(), Some("CORTES"));
}

#[test]
fn test_resolve_oc_name_unresolvable_is_none() {
    // A name reference with no resources in scope yields None (the path
    // is left unlabelled) rather than an error. ~keep
    let doc = oc_test_doc();
    let name_ref = Object::Name("MC9".to_string());
    assert_eq!(doc.resolve_oc_layer_name(None, &name_ref), None);
}

#[test]
fn test_extract_paths_layer_none_for_plain_stroke() {
    // End-to-end through the real page pipeline: a stroked line on a page
    // with no optional content yields a path whose `layer` is None. Guards
    // the page-level marked-content refactor against perturbing plain
    // extraction (and mirrors the Python shape test's synthetic PDF). ~keep
    let doc = PdfDocument::from_bytes(build_minimal_pdf(b"100 100 m 200 200 l S")).unwrap();
    let paths = doc.extract_paths(0).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].layer, None);
}

/// Like [`build_minimal_pdf`] but with a caller-supplied `/MediaBox` array
/// literal (e.g. `"10 -100 622 692"`), for GH#1653 extent-vs-corner tests. ~keep
fn build_minimal_pdf_with_media_box(media_box: &str, content: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [{media_box}] /Contents 4 0 R /Resources << >> >>\nendobj\n"
        )
        .as_bytes(),
    );

    let off4 = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off4).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

#[test]
fn test_extract_page_text_dimensions_use_extent_not_corner_for_nonzero_origin() {
    // GH#1653: `/MediaBox [10 -100 622 692]` describes a 612x792 page whose
    // lower-left corner sits at (10, -100), not the origin. `page_width` /
    // `page_height` must report the EXTENT (urx - llx, ury - lly) = 612x792,
    // not the raw upper-right corner (622, 692). ~keep
    let pdf = build_minimal_pdf_with_media_box("10 -100 622 692", b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc.extract_page_text(0).unwrap();
    assert!(
        (page_text.page_width - 612.0).abs() < 0.1,
        "expected extent width 612, got {}",
        page_text.page_width
    );
    assert!(
        (page_text.page_height - 792.0).abs() < 0.1,
        "expected extent height 792, got {}",
        page_text.page_height
    );
}

#[test]
fn test_extract_page_text_dimensions_unchanged_for_zero_origin() {
    // Companion to the non-zero-origin test above: when llx == lly == 0 (the
    // overwhelming majority of real PDFs), extent and corner coincide, so this
    // must report the exact same values as before GH#1653's fix. ~keep
    let pdf = build_minimal_pdf_with_media_box("0 0 612 792", b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page_text = doc.extract_page_text(0).unwrap();
    assert!((page_text.page_width - 612.0).abs() < 0.1);
    assert!((page_text.page_height - 792.0).abs() < 0.1);
}

/// Builds a single-page PDF whose `/Rotate` entry is the caller-supplied raw
/// PDF token (`"90"`, `"-90"`, `"135"`, `"90.5"`, `"(bogus)"`, ...), placed on
/// the page dict (`inherited = false`) or the parent `/Pages` node
/// (`inherited = true`). Mirrors `xberg::pdf::render::build_pdf_with_rotate`'s
/// fixture shape (out of scope to reuse directly - that file is owned by a
/// concurrent change), built with this crate's own manual PDF writer instead
/// of `lopdf`, which this crate does not depend on. ~keep
fn build_pdf_with_rotate_token(rotate_token: Option<&str>, inherited: bool) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let page_rotate = if inherited {
        String::new()
    } else {
        rotate_token.map_or(String::new(), |t| format!(" /Rotate {t}"))
    };
    let pages_rotate = if inherited {
        rotate_token.map_or(String::new(), |t| format!(" /Rotate {t}"))
    } else {
        String::new()
    };

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(
        format!("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1{pages_rotate} >>\nendobj\n").as_bytes(),
    );

    let off3 = pdf.len();
    pdf.extend_from_slice(
        format!("3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100]{page_rotate} >>\nendobj\n").as_bytes(),
    );

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in [off1, off2, off3] {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes());

    pdf
}

#[test]
fn test_get_page_rotation_status_absent_is_absent() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(None, false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Absent);
}

#[test]
fn test_get_page_rotation_status_zero_is_valid_zero() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("0"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(0));
}

#[test]
fn test_get_page_rotation_status_90_is_valid_90() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("90"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(90));
}

#[test]
fn test_get_page_rotation_status_180_is_valid_180() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("180"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(180));
}

#[test]
fn test_get_page_rotation_status_270_is_valid_270() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("270"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(270));
}

#[test]
fn test_get_page_rotation_status_negative_90_normalizes_to_270() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("-90"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(270));
}

#[test]
fn test_get_page_rotation_status_135_is_malformed_not_folded_to_valid_zero() {
    // GH#1654: `get_page_rotation` folds this to plain `0`, indistinguishable
    // from a genuine `/Rotate 0` or an absent entry. The new accessor must
    // distinguish it as Malformed instead. ~keep
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("135"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Malformed);
}

#[test]
fn test_get_page_rotation_status_real_value_is_read() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("90.0"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(90));
}

#[test]
fn test_get_page_rotation_status_non_integral_real_is_malformed() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("90.5"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Malformed);
}

#[test]
fn test_get_page_rotation_status_non_numeric_is_malformed() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("(bogus)"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Malformed);
}

#[test]
fn test_get_page_rotation_status_inherited_from_pages_node() {
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("90"), true)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Valid(90));
}

#[test]
fn test_get_page_rotation_still_folds_absent_and_malformed_to_zero() {
    // Pins get_page_rotation's existing fold-to-0 contract, which must not
    // change: both absent and out-of-spec /Rotate values still read as 0
    // through the original accessor. ~keep
    let absent = PdfDocument::from_bytes(build_pdf_with_rotate_token(None, false)).unwrap();
    assert_eq!(absent.get_page_rotation(0).unwrap(), 0);
    let malformed = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("135"), false)).unwrap();
    assert_eq!(malformed.get_page_rotation(0).unwrap(), 0);
}
