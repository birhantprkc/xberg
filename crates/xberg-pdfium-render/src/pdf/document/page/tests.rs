use crate::prelude::*;
use crate::utils::test::{test_bind_to_pdfium, test_fixture_path};
use image_025::{GenericImageView, ImageFormat};

#[test]
fn test_page_rendering_reusing_bitmap() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file(&test_fixture_path("export-test.pdf"), None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2000)
        .set_maximum_height(2000)
        .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

    let mut bitmap = PdfBitmap::empty(2500, 2500, PdfBitmapFormat::default(), pdfium.bindings())?;

    for (index, page) in document.pages().iter().enumerate() {
        page.render_into_bitmap_with_config(&mut bitmap, &render_config)?;

        bitmap
            .as_image()?
            .into_rgb8()
            .save_with_format(format!("test-page-{}.jpg", index), ImageFormat::Jpeg)
            .map_err(|_| PdfiumError::ImageError)?;
    }

    Ok(())
}

#[test]
fn test_rendered_image_dimension() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file(&test_fixture_path("dimensions-test.pdf"), None)?;

    let render_config = PdfRenderConfig::new().set_target_width(500).set_maximum_height(500);

    for page in document.pages().iter() {
        let rendered_page = page.render_with_config(&render_config)?.as_image()?;

        let (width, _height) = rendered_page.dimensions();

        assert_eq!(width, 500);
    }

    Ok(())
}
