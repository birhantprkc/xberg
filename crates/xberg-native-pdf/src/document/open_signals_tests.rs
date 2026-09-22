//! Regression coverage for `gather_page_signals`' image classification (GH#1732).
//!
//! Lives beside `open.rs` rather than in `document/tests.rs` (322 KB, close to the
//! project's 500 KiB file-safety limit) for the same reason `pdf/native/images/
//! parallel_tests.rs` gives in the `xberg` crate: it keeps that file from growing
//! further while still sitting next to the code it covers.

use super::PdfDocument;
use crate::extractors::auto::ImageCodecClass;
use crate::extractors::images::decode_call_count;

/// A one-page PDF, `page_side`×`page_side`, with a single uncompressed DeviceRGB
/// image XObject painted at `(inset, inset)` with size `image_side`×`image_side`
/// via `cm`. Uncompressed (no `/Filter`) so the image decodes through the same
/// per-pixel raw-buffer path CCITT/JBIG2/Flate images do (the expensive path
/// GH#1732 is about), without needing a real compressed codestream fixture.
fn build_single_page_pdf_with_raw_image(page_side: u32, image_side: u32, inset: u32) -> Vec<u8> {
    let mut buf = Vec::<u8>::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::new();

    offsets.push(buf.len());
    buf.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

    offsets.push(buf.len());
    buf.extend_from_slice(b"2 0 obj\n<</Type /Pages /Kids [3 0 R] /Count 1>>\nendobj\n");

    offsets.push(buf.len());
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<</Type /Page /Parent 2 0 R /MediaBox [0 0 {page_side} {page_side}] \
             /Resources <</XObject <</Im0 5 0 R>>>> /Contents 4 0 R>>\nendobj\n"
        )
        .as_bytes(),
    );

    let content = format!("q {image_side} 0 0 {image_side} {inset} {inset} cm /Im0 Do Q");
    offsets.push(buf.len());
    buf.extend_from_slice(format!("4 0 obj\n<</Length {}>>\nstream\n", content.len()).as_bytes());
    buf.extend_from_slice(content.as_bytes());
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    let pixel_count = (image_side as usize) * (image_side as usize) * 3;
    offsets.push(buf.len());
    buf.extend_from_slice(
        format!(
            "5 0 obj\n<</Type /XObject /Subtype /Image /Width {image_side} /Height {image_side} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {pixel_count}>>\nstream\n"
        )
        .as_bytes(),
    );
    buf.extend((0..pixel_count).map(|i| ((i * 37 + i / 7) % 251) as u8));
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = buf.len();
    let total_objs = 6;
    buf.extend_from_slice(b"xref\n");
    buf.extend_from_slice(format!("0 {total_objs}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(format!("trailer\n<</Size {total_objs} /Root 1 0 R>>\n").as_bytes());
    buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    buf
}

/// `classify_page` (via `gather_page_signals`) must decode zero embedded images: it
/// only needs bbox + filter-chain metadata, both available from the cheap Phase 1
/// `page_image_handles` walk. Before the GH#1732 fix this called `extract_images`,
/// which fully decoded every embedded image just to classify its codec.
///
/// Asserts a *delta* around each call rather than an absolute counter value so the
/// test is immune to other tests incrementing the same process-wide-if-it-were-an-
/// AtomicUsize counter; `decode_call_count` is thread-local instead; see its doc
/// comment. The second half proves the counter itself is wired to real decode work
/// (`extract_images` must move it) rather than reading 0 vacuously either way. ~keep
#[test]
fn classify_page_does_not_decode_embedded_images() {
    let pdf = build_single_page_pdf_with_raw_image(200, 150, 25);
    let doc = PdfDocument::from_bytes(pdf).expect("fixture must open");

    let before_classify = decode_call_count();
    let classification = doc.classify_page(0).expect("classify_page must succeed");
    let after_classify = decode_call_count();
    assert_eq!(
        after_classify,
        before_classify,
        "classify_page must not decode any embedded image, got a decode-count delta of {}",
        after_classify - before_classify
    );

    // The page's image still contributes its area/codec to the signals -- avoiding
    // the decode must not make the image invisible to classification.
    assert_eq!(classification.signals.codec, ImageCodecClass::Other);

    let before_extract = decode_call_count();
    let images = doc.extract_images(0).expect("extract_images must succeed");
    let after_extract = decode_call_count();
    assert_eq!(images.len(), 1, "fixture has exactly one embedded image");
    assert_eq!(
        after_extract - before_extract,
        1,
        "extract_images must decode exactly the one embedded image on the page \
         (proves decode_call_count is wired to real decode work, not vacuously 0)"
    );
}

/// `image_area_ratio` computed from the Phase 1 handle's `bbox` (GH#1732's
/// no-decode path) must equal the value the old full-decode path produced: the
/// image is painted at (25,25) sized 150x150 on a 200x200 page, so the
/// intersection area is exactly 150*150 = 22500 and the ratio is 22500/40000.
#[test]
fn classify_page_image_area_ratio_matches_expected_bbox_intersection() {
    let pdf = build_single_page_pdf_with_raw_image(200, 150, 25);
    let doc = PdfDocument::from_bytes(pdf).expect("fixture must open");

    let classification = doc.classify_page(0).expect("classify_page must succeed");
    let expected_ratio = (150.0 * 150.0) / (200.0 * 200.0);
    assert!(
        (classification.signals.image_area_ratio - expected_ratio).abs() < 1e-5,
        "expected image_area_ratio {expected_ratio}, got {}",
        classification.signals.image_area_ratio
    );
}

/// A one-page PDF with no `/XObject` resource and an empty content stream.
fn build_single_page_pdf_without_images(page_side: u32) -> Vec<u8> {
    let mut buf = Vec::<u8>::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::new();

    offsets.push(buf.len());
    buf.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

    offsets.push(buf.len());
    buf.extend_from_slice(b"2 0 obj\n<</Type /Pages /Kids [3 0 R] /Count 1>>\nendobj\n");

    offsets.push(buf.len());
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<</Type /Page /Parent 2 0 R /MediaBox [0 0 {page_side} {page_side}] \
             /Resources <<>> /Contents 4 0 R>>\nendobj\n"
        )
        .as_bytes(),
    );

    offsets.push(buf.len());
    buf.extend_from_slice(b"4 0 obj\n<</Length 0>>\nstream\n\nendstream\nendobj\n");

    let xref_offset = buf.len();
    let total_objs = 5;
    buf.extend_from_slice(b"xref\n");
    buf.extend_from_slice(format!("0 {total_objs}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(format!("trailer\n<</Size {total_objs} /Root 1 0 R>>\n").as_bytes());
    buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    buf
}

/// A page with no embedded images must still classify without decoding anything,
/// and must report `ImageCodecClass::None` and a zero image area ratio.
#[test]
fn classify_page_with_no_images_reports_none_codec() {
    let pdf = build_single_page_pdf_without_images(200);
    let doc = PdfDocument::from_bytes(pdf).expect("fixture must open");

    let before = decode_call_count();
    let classification = doc.classify_page(0).expect("classify_page must succeed");
    let after = decode_call_count();
    assert_eq!(after, before, "a page with no images must still decode nothing");
    assert_eq!(classification.signals.codec, ImageCodecClass::None);
    assert_eq!(classification.signals.image_area_ratio, 0.0);
}
