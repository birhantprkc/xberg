use super::*;
use crate::prelude::*;
use crate::utils::test::{test_bind_to_pdfium, test_fixture_path};

#[test]
fn test_page_image_object_retains_format() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let image = pdfium
        .load_pdf_from_file(&test_fixture_path("path-test.pdf"), None)?
        .pages()
        .get(0)?
        .render_with_config(&PdfRenderConfig::new().set_target_width(1000))?
        .as_image()?;

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document.pages_mut().create_page_at_end(PdfPagePaperSize::a4())?;

    let object = page.objects_mut().create_image_object(
        PdfPoints::new(100.0),
        PdfPoints::new(100.0),
        &image,
        Some(PdfPoints::new(image.width() as f32)),
        Some(PdfPoints::new(image.height() as f32)),
    )?;

    let raw_image = object.as_image_object().unwrap().get_raw_image()?;

    let processed_image = object.as_image_object().unwrap().get_processed_image(&document)?;

    assert!(compare_equality_of_byte_arrays(
        image.as_bytes(),
        raw_image.into_rgba8().as_raw().as_slice()
    ));

    assert!(compare_equality_of_byte_arrays(
        image.as_bytes(),
        processed_image.into_rgba8().as_raw().as_slice()
    ));

    Ok(())
}

fn compare_equality_of_byte_arrays(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for index in 0..a.len() {
        if a[index] != b[index] {
            return false;
        }
    }

    true
}

#[test]
fn test_image_scaling_keeps_aspect_ratio() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document.pages_mut().create_page_at_end(PdfPagePaperSize::a4())?;

    let image = DynamicImage::new_rgb8(100, 200);

    let object = page.objects_mut().create_image_object(
        PdfPoints::new(0.0),
        PdfPoints::new(0.0),
        &image,
        Some(PdfPoints::new(image.width() as f32)),
        Some(PdfPoints::new(image.height() as f32)),
    )?;

    let image_object = object.as_image_object().unwrap();

    assert_eq!(
        image_object.get_processed_bitmap_with_width(&document, 50)?.height(),
        100
    );
    assert_eq!(
        image_object.get_processed_image_with_width(&document, 50)?.height(),
        100
    );
    assert_eq!(
        image_object.get_processed_bitmap_with_height(&document, 50)?.width(),
        25
    );
    assert_eq!(image_object.get_processed_image_with_height(&document, 50)?.width(), 25);

    Ok(())
}
