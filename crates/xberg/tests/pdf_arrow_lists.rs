//! Arrow bullets must produce list items through the public extractor (#1790).
#![cfg(feature = "pdf")]

mod helpers;
use helpers::extract_bytes_document_blocking;
use xberg::core::config::{ExtractionConfig, OutputFormat, PdfConfig};
use xberg::types::document_structure::NodeContent;

fn list_pdf(marker: u16) -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 720 Td (Fill in these fields:) Tj ET\n\
        BT /F2 12 Tf 72 700 Td (A) Tj /F1 12 Tf ( First field value) Tj ET\n\
        BT /F2 12 Tf 72 680 Td (A) Tj /F1 12 Tf ( Second field value) Tj ET";
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        /CMapName /Bullet def /CMapType 2 def\n\
        1 begincodespacerange <00> <FF> endcodespacerange\n\
        1 beginbfchar <41> <{marker:04X}> endbfchar\n\
        endcmap CMapName currentdict /CMap defineresource pop end end"
    );
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            String::from_utf8_lossy(content)
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 7 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len()),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 8\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    pdf
}

#[test]
fn arrow_and_round_bullets_become_list_items() {
    for marker in [0x27A2, 0x2022] {
        let config = ExtractionConfig {
            disable_ocr: true,
            use_cache: false,
            enable_quality_processing: false,
            include_document_structure: true,
            output_format: OutputFormat::Markdown,
            pdf_options: Some(PdfConfig {
                extract_tables: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = extract_bytes_document_blocking(&list_pdf(marker), "application/pdf", &config).unwrap();
        assert!(result.tables.is_empty());
        let document = result.document.as_ref().unwrap();
        let items: Vec<_> = document
            .nodes
            .iter()
            .filter_map(|node| match &node.content {
                NodeContent::ListItem { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(items, ["First field value", "Second field value"], "{}", result.content);
        assert!(result.content.contains("- First field value"), "{}", result.content);
        assert!(result.content.contains("- Second field value"), "{}", result.content);
        assert!(!result.content.contains('➢'));
    }
}
