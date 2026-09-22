//! GH#1732: a full-page image on a page that already has a usable text layer, decoded and PNG
//! re-encoded only to be dropped once OCR excludes it, must instead skip its decode entirely
//! when nothing but OCR would ever read its bytes.
//!
//! Lives beside `images.rs` rather than in its `tests` module so that file stays inside the
//! project's file-length limit, the same reason `parallel_tests.rs` is a sibling file.
//!
//! `ocr_skip_candidate_pages`/`covers_full_page` only exist in their real form under this
//! feature combination (see their definitions in `images.rs`), matching the gate on
//! `should_skip_pdf_image_ocr`/`drop_ocr_only_images` this module anticipates. ~keep

use super::*;
use crate::types::{PageDimensions, PageInfo, PageStructure, PageUnitType};

const PAGE_WIDTH: f32 = 100.0;
const PAGE_HEIGHT: f32 = 100.0;
const IMAGE_SIDE_PX: u32 = 4;

/// A single-page PDF whose page carries both real text content and a full-page, unfiltered
/// (raw pixel) grayscale image XObject -- the exact combination GH#1732 describes: a full-page
/// raster on a page that already has a native text layer. The image has no `/Filter`, so
/// `xberg_native_pdf` decodes it as `ImageData::Raw { format: PixelFormat::Grayscale, .. }`,
/// which is exactly what `raw_pixels_to_png` re-encodes.
fn build_full_page_image_with_text_pdf() -> Vec<u8> {
    let pixels = vec![0x40u8; (IMAGE_SIDE_PX * IMAGE_SIDE_PX) as usize];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
             /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> \
             /XObject << /Im0 6 0 R >> >> >>\nendobj\n"
        )
        .as_bytes(),
    );

    let stream =
        format!("q {PAGE_WIDTH} 0 0 {PAGE_HEIGHT} 0 0 cm /Im0 Do Q\nBT /F1 10 Tf 10 10 Td (native text) Tj ET\n");
    offsets.push(pdf.len());
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{stream}\nendstream\nendobj\n",
            stream.len()
        )
        .as_bytes(),
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
          /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XObject /Subtype /Image /Width {IMAGE_SIDE_PX} \
             /Height {IMAGE_SIDE_PX} /ColorSpace /DeviceGray /BitsPerComponent 8 \
             /Length {} >>\nstream\n",
            pixels.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&pixels);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_pos = pdf.len();
    let total_objs = offsets.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {total_objs}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f\r\n");
    for &off in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n\r\n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {total_objs} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );
    pdf
}

/// The `PageStructure` `ocr_skip_candidate_pages` reads: page 1, non-blank, with the fixture's
/// exact `MediaBox` dimensions.
fn full_page_candidate_structure() -> PageStructure {
    PageStructure {
        total_count: 1,
        unit_type: PageUnitType::Page,
        boundaries: None,
        pages: Some(vec![PageInfo {
            number: 1,
            title: None,
            dimensions: Some(PageDimensions {
                width: PAGE_WIDTH as f64,
                height: PAGE_HEIGHT as f64,
            }),
            image_count: None,
            table_count: None,
            hidden: None,
            is_blank: Some(false),
            has_vector_graphics: false,
        }]),
    }
}

#[test]
fn test_ocr_skip_candidate_pages_selects_only_non_blank_dimensioned_pages() {
    let structure = PageStructure {
        total_count: 3,
        unit_type: PageUnitType::Page,
        boundaries: None,
        pages: Some(vec![
            PageInfo {
                number: 1,
                title: None,
                dimensions: Some(PageDimensions {
                    width: 100.0,
                    height: 200.0,
                }),
                image_count: None,
                table_count: None,
                hidden: None,
                is_blank: Some(false),
                has_vector_graphics: false,
            },
            PageInfo {
                number: 2,
                title: None,
                dimensions: Some(PageDimensions {
                    width: 100.0,
                    height: 200.0,
                }),
                image_count: None,
                table_count: None,
                hidden: None,
                is_blank: Some(true),
                has_vector_graphics: false,
            },
            PageInfo {
                number: 3,
                title: None,
                dimensions: None,
                image_count: None,
                table_count: None,
                hidden: None,
                is_blank: Some(false),
                has_vector_graphics: false,
            },
        ]),
    };

    let candidates = ocr_skip_candidate_pages(Some(&structure));

    assert_eq!(
        candidates,
        std::collections::HashMap::from([(1u32, (100.0, 200.0))]),
        "only page 1 (non-blank, dimensioned) must be a candidate; got {candidates:?}"
    );
}

#[test]
fn test_ocr_skip_candidate_pages_empty_without_page_structure() {
    assert!(
        ocr_skip_candidate_pages(None).is_empty(),
        "no page structure means no candidate pages"
    );
}

#[test]
fn test_covers_full_page_true_for_full_page_bbox() {
    let bbox = xberg_native_pdf::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    assert!(covers_full_page(&bbox, 100.0, 100.0));
}

#[test]
fn test_covers_full_page_false_below_area_ratio() {
    // 50×100 covers exactly half the 100×100 page -- well under the 0.85 ratio.
    let bbox = xberg_native_pdf::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 50.0,
        height: 100.0,
    };
    assert!(!covers_full_page(&bbox, 100.0, 100.0));
}

#[test]
fn test_covers_full_page_false_for_zero_page_dimensions() {
    let bbox = xberg_native_pdf::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    assert!(!covers_full_page(&bbox, 0.0, 100.0));
    assert!(!covers_full_page(&bbox, 100.0, 0.0));
}

#[test]
fn test_extract_images_with_data_decodes_full_page_image_without_candidate_pages() {
    let pdf = build_full_page_image_with_text_pdf();
    let mut doc = crate::pdf::native::NativeDocument::open_bytes(&pdf).expect("fixture must open");

    let (result, warnings) = extract_images_with_data(&mut doc, None, None, &std::collections::HashMap::new())
        .expect("extraction must not error");

    assert_eq!(result.len(), 1, "fixture must yield exactly one image");
    assert_eq!(warnings.len(), 0);
    assert_eq!(
        result[0].format.as_ref(),
        "png",
        "an empty candidate map must decode and PNG re-encode the image exactly as before GH#1732's fix"
    );
    assert!(
        !result[0].data.is_empty(),
        "without a candidate map the image must be fully decoded"
    );
}

#[test]
fn test_extract_images_with_data_skips_decode_for_ocr_only_full_page_image() {
    let pdf = build_full_page_image_with_text_pdf();
    let mut doc = crate::pdf::native::NativeDocument::open_bytes(&pdf).expect("fixture must open");
    let candidates = ocr_skip_candidate_pages(Some(&full_page_candidate_structure()));

    let (result, warnings) =
        extract_images_with_data(&mut doc, None, None, &candidates).expect("extraction must not error");

    assert_eq!(
        result.len(),
        1,
        "the image must still appear in the result, metadata-only"
    );
    assert_eq!(
        warnings.len(),
        0,
        "skipping a decode is not a failure and must not warn"
    );

    let image = &result[0];
    assert!(image.data.is_empty(), "data must stay empty when the decode is skipped");
    assert_eq!(image.format.as_ref(), "skipped");
    assert_eq!(image.page_number, Some(1));
    assert_eq!(image.image_index, 0);
    assert_eq!(image.width, Some(IMAGE_SIDE_PX));
    assert_eq!(image.height, Some(IMAGE_SIDE_PX));
    let bbox = image
        .bounding_box
        .expect("skipped image must still carry its bounding box");
    assert_eq!((bbox.x0, bbox.y0), (0.0, 0.0));
    assert_eq!((bbox.x1, bbox.y1), (PAGE_WIDTH as f64, PAGE_HEIGHT as f64));
}
