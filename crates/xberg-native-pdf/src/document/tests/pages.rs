use super::super::*;
use super::pdf_fixtures::*;

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
fn test_is_cm_or_symbol_font() {
    assert!(PdfDocument::is_cm_or_symbol_font("ABCDEF+CMSY10"));
    assert!(PdfDocument::is_cm_or_symbol_font("CMR12"));
    assert!(PdfDocument::is_cm_or_symbol_font("Symbol"));
    assert!(!PdfDocument::is_cm_or_symbol_font("ABCDEF+Helvetica"));
    assert!(!PdfDocument::is_cm_or_symbol_font("TimesNewRoman"));
}

#[test]
fn test_get_page_rotation_status_135_is_malformed_not_folded_to_valid_zero() {
    // GH#1654: `get_page_rotation` folds this to plain `0`, indistinguishable
    // from a genuine `/Rotate 0` or an absent entry. The new accessor must
    // distinguish it as Malformed instead. ~keep
    let doc = PdfDocument::from_bytes(build_pdf_with_rotate_token(Some("135"), false)).unwrap();
    assert_eq!(doc.get_page_rotation_status(0).unwrap(), PageRotation::Malformed);
}
