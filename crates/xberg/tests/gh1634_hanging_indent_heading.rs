//! GH#1634: a hanging-number heading swallowed the body beneath it when the
//! body was indented to the title's left edge.
//!
//! `heading_continuation_is_hanging_indent` treats a line as the continuation
//! of a numbered heading when the heading has a hanging indent (title right of
//! number) and the line starts at the title's edge. That is the geometry of a
//! manual whose body returns to the margin — and equally the geometry of every
//! layout that indents the *whole clause*, which is how contracts, tenders and
//! a large family of installation manuals are set.
//!
//! Geometry transcribed from the issue: A4, Helvetica 9.36pt, one size and one
//! weight throughout, number at x 104.4 and title at x 161.1.
#![cfg(feature = "pdf")]
#![allow(clippy::print_stdout)]

mod helpers;
use helpers::extract_uri_document_blocking;
use xberg::core::config::{ExtractionConfig, OutputFormat};

const NUMBER_X: f32 = 104.4;
const TITLE_X: f32 = 161.1;
const MARGIN_X: f32 = 104.4;

fn text_at(x: f32, y: f32, size: f32, text: &str) -> String {
    format!("BT /F1 {size} Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET\n")
}

/// One A4 page per content stream, Helvetica, no embedded font.
fn build_pdf(pages: &[String]) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..pages.len()).map(|index| format!("{} 0 R", 4 + 2 * index)).collect();
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    for page in pages {
        let contents_id = objects.len() + 2;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.32 841.92] /Contents {contents_id} 0 R \
                 /Resources << /Font << /F1 3 0 R >> >> >>"
            )
            .into_bytes(),
        );
        objects.push(
            [
                format!("<< /Length {} >>\nstream\n", page.len()).into_bytes(),
                page.clone().into_bytes(),
                b"endstream".to_vec(),
            ]
            .concat(),
        );
    }

    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn extract(label: &str, pages: &[String]) -> String {
    // Per-call directory: the tests run in parallel and a shared path let one
    // overwrite the other's fixture, so both asserted against the same page. ~keep
    let dir = std::env::temp_dir().join(format!("gh1634-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("gh1634.pdf");
    std::fs::write(&path, build_pdf(pages)).expect("write fixture");
    let config = ExtractionConfig {
        output_format: OutputFormat::Markdown,
        use_cache: false,
        ..Default::default()
    };
    let result = extract_uri_document_blocking(&path, None, &config).expect("fixture must extract");
    let _ = std::fs::remove_dir_all(&dir);
    result.content
}

/// Body indented to the title's left edge — the heading must still close.
fn page_body_at_title_edge() -> String {
    text_at(NUMBER_X, 314.0, 9.36, "3.1.7")
        + &text_at(TITLE_X, 314.0, 9.36, "Innovatie/ontwikkelingen")
        + &text_at(
            TITLE_X,
            302.0,
            9.36,
            "Wat zijn de toekomstige ontwikkelingen en te verwachten innovaties",
        )
        + &text_at(
            TITLE_X,
            290.0,
            9.36,
            "op het gebied van duurzame installatietechniek binnen uw organisatie",
        )
}

/// A genuine two-line heading wrap, then body at the margin.
fn page_wrapped_heading() -> String {
    text_at(NUMBER_X, 314.0, 9.36, "4.8.3")
        + &text_at(TITLE_X, 314.0, 9.36, "Dakuitmonding combidoorvoer-verticaal en")
        + &text_at(TITLE_X, 302.0, 9.36, "dubbelpijpsdoorvoer-verticaal")
        + &text_at(
            MARGIN_X,
            290.0,
            9.36,
            "Werkingsprincipe Indien de kamerthermostaat warmte vraagt zal de",
        )
        + &text_at(
            MARGIN_X,
            278.0,
            9.36,
            "ventilator gaan draaien en wordt de verbrandingslucht aangezogen",
        )
}

#[test]
fn a_single_line_heading_does_not_swallow_body_indented_to_its_title_edge() {
    let content = extract("body-at-title-edge", &[page_body_at_title_edge()]);
    println!("--- extracted ---\n{content}\n---");
    assert!(
        content.contains("Innovatie/ontwikkelingen"),
        "the heading text is missing entirely: {content}"
    );
    assert!(
        !content.contains("Innovatie/ontwikkelingen Wat zijn"),
        "the heading swallowed the body beneath it — they must be separate elements:\n{content}"
    );
}

#[test]
fn a_wrapped_heading_still_closes_before_the_body() {
    let content = extract("wrapped-heading", &[page_wrapped_heading()]);
    println!("--- extracted ---\n{content}\n---");
    assert!(
        content.contains("dubbelpijpsdoorvoer-verticaal"),
        "the wrapped heading's second line is missing: {content}"
    );
    assert!(
        !content.contains("dubbelpijpsdoorvoer-verticaal Werkingsprincipe"),
        "the wrapped heading was never closed and pulled the body in after it:\n{content}"
    );
}

/// Flush-left prose that merely begins with a decimal figure. It is not a
/// heading and must not be split, however many lines it runs to.
fn page_decimal_prefixed_prose() -> String {
    text_at(
        MARGIN_X,
        314.0,
        9.36,
        "3.2 million users were affected across the region this quarter,",
    ) + &text_at(
        MARGIN_X,
        302.0,
        9.36,
        "according to figures published by the regulator on Tuesday",
    ) + &text_at(MARGIN_X, 290.0, 9.36, "after a review.")
}

#[test]
fn prose_beginning_with_a_decimal_figure_is_not_split_as_a_heading() {
    let content = extract("decimal-prose", &[page_decimal_prefixed_prose()]);
    println!("--- extracted ---\n{content}\n---");
    assert!(
        content.contains("according to figures published by the regulator on Tuesday after a review"),
        "flush-left prose starting with a decimal figure was split as though it were a numbered \
         heading -- is_numbered_section_heading returns true for any two-level number:\n{content}"
    );
}
