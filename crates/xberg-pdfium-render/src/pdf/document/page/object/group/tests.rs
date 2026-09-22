use crate::prelude::*;
use crate::utils::test::{test_bind_to_pdfium, test_fixture_path};

#[test]
fn test_group_bounds() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file(&test_fixture_path("export-test.pdf"), None)?;

    let page = document.pages().get(2)?;

    let mut group = page.objects().create_empty_group();

    group.append(
        page.objects()
            .iter()
            .filter(|object| {
                object.object_type() == PdfPageObjectType::Text
                    && object.bounds().unwrap().bottom() > page.height() / 2.0
            })
            .collect::<Vec<_>>()
            .as_mut_slice(),
    )?;

    let bounds = group.bounds()?;

    assert_eq!(bounds.bottom().value, 428.31033);
    assert_eq!(bounds.left().value, 62.60526);
    assert_eq!(bounds.top().value, 807.8812);
    assert_eq!(bounds.right().value, 544.48096);

    Ok(())
}

#[test]
fn test_group_text() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file(&test_fixture_path("export-test.pdf"), None)?;

    let page = document.pages().get(5)?;

    let mut group = page.objects().create_empty_group();

    group.append(
        page.objects()
            .iter()
            .filter(|object| {
                object.object_type() == PdfPageObjectType::Text
                    && object.bounds().unwrap().bottom() < page.height() / 2.0
            })
            .collect::<Vec<_>>()
            .as_mut_slice(),
    )?;

    assert_eq!(
        group.text_separated(" "),
        "Cento Concerti Ecclesiastici a Una, a Due, a Tre, e   a Quattro voci Giacomo Vincenti, Venice, 1605 Edited by Alastair Carey Source is the 1605 reprint of the original 1602 publication.  Item #2 in the source. Folio pages f5r (binding B1) in both Can to and Basso partbooks. The Basso partbook is barred; the Canto par tbook is not. The piece is marked ™Canto solo, Û Tenoreº in the  Basso partbook, indicating it can be sung either by a Soprano or by a  Tenor down an octave. V.  Quem vidistis, pastores, dicite, annuntiate nobis: in terris quis apparuit? R.  Natum vidimus, et choros angelorum collaudantes Dominum. Alleluia. What did you see, shepherds, speak, tell us: who has appeared on earth? We saw the new-born, and choirs of angels praising the Lord. Alleluia. Third responsory at Matins on Christmas Day 2  Basso, bar 47: one tone lower in source."
    );

    Ok(())
}

#[test]
fn test_group_apply() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document.pages_mut().create_page_at_start(PdfPagePaperSize::a4())?;

    page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(100.0, 100.0, 200.0, 200.0),
        None,
        None,
        Some(PdfColor::RED),
    )?;

    page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(150.0, 150.0, 250.0, 250.0),
        None,
        None,
        Some(PdfColor::GREEN),
    )?;

    page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(200.0, 200.0, 300.0, 300.0),
        None,
        None,
        Some(PdfColor::BLUE),
    )?;

    let mut group = PdfPageGroupObject::new(&page, |_| true)?;

    let bounds = group.bounds()?;

    assert_eq!(bounds.bottom().value, 100.0);
    assert_eq!(bounds.left().value, 100.0);
    assert_eq!(bounds.top().value, 300.0);
    assert_eq!(bounds.right().value, 300.0);

    group.translate(PdfPoints::new(150.0), PdfPoints::new(200.0))?;

    let bounds = group.bounds()?;

    assert_eq!(bounds.bottom().value, 300.0);
    assert_eq!(bounds.left().value, 250.0);
    assert_eq!(bounds.top().value, 500.0);
    assert_eq!(bounds.right().value, 450.0);

    Ok(())
}
