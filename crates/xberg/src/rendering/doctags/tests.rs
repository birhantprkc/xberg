use super::*;
use crate::types::internal_builder::InternalDocumentBuilder;

fn image(description: Option<&str>) -> crate::types::ExtractedImage {
    crate::types::ExtractedImage {
        data: bytes::Bytes::new(),
        format: std::borrow::Cow::Borrowed("png"),
        image_index: 0,
        page_number: None,
        width: None,
        height: None,
        colorspace: None,
        bits_per_component: None,
        is_mask: false,
        description: description.map(str::to_string),
        ocr_result: None,
        bounding_box: None,
        source_path: None,
        image_kind: None,
        kind_confidence: None,
        cluster_id: None,
        caption: None,
        qr_codes: None,
        data_base64: None,
    }
}

fn bbox(x0: f64, y0: f64, x1: f64, y1: f64) -> BoundingBox {
    BoundingBox { x0, y0, x1, y1 }
}

/// US Letter, chosen so the grid arithmetic lands on round numbers.
const PAGE_W: f64 = 612.0;
const PAGE_H: f64 = 792.0;

fn with_page_dims(mut doc: InternalDocument, dimensions: Option<(f64, f64)>) -> InternalDocument {
    doc.metadata.pages = Some(crate::types::PageStructure {
        total_count: 1,
        unit_type: crate::types::PageUnitType::Page,
        boundaries: None,
        pages: Some(vec![crate::types::PageInfo {
            number: 1,
            title: None,
            dimensions: dimensions.map(Into::into),
            image_count: None,
            table_count: None,
            hidden: None,
            is_blank: None,
            has_vector_graphics: false,
        }]),
    });
    doc
}

#[test]
fn test_loc_tokens_flip_the_vertical_axis() {
    // Box spanning the top-left quarter of the page.
    let top_left = loc_tokens(&bbox(61.2, 396.0, 306.0, 792.0), (PAGE_W, PAGE_H)).unwrap();
    assert_eq!(top_left, "<loc_50><loc_0><loc_250><loc_250>");

    // The same box moved to the bottom half: PDF y grows upward, DocTags
    // downward, so the tokens must increase rather than stay put.
    let bottom_left = loc_tokens(&bbox(61.2, 0.0, 306.0, 396.0), (PAGE_W, PAGE_H)).unwrap();
    assert_eq!(bottom_left, "<loc_50><loc_250><loc_250><loc_500>");
}

#[test]
fn test_loc_tokens_clamp_out_of_page_boxes() {
    let out = loc_tokens(&bbox(-100.0, -100.0, PAGE_W * 2.0, PAGE_H * 2.0), (PAGE_W, PAGE_H)).unwrap();
    assert_eq!(out, "<loc_0><loc_0><loc_500><loc_500>");
}

#[test]
fn test_loc_tokens_tolerate_reversed_corners() {
    let forward = loc_tokens(&bbox(61.2, 396.0, 306.0, 792.0), (PAGE_W, PAGE_H)).unwrap();
    let reversed = loc_tokens(&bbox(306.0, 792.0, 61.2, 396.0), (PAGE_W, PAGE_H)).unwrap();
    assert_eq!(forward, reversed);
}

#[test]
fn test_loc_tokens_reject_non_finite_coordinates() {
    assert!(loc_tokens(&bbox(f64::NAN, 0.0, 10.0, 10.0), (PAGE_W, PAGE_H)).is_none());
    assert!(loc_tokens(&bbox(0.0, 0.0, f64::INFINITY, 10.0), (PAGE_W, PAGE_H)).is_none());
}

/// Every other geometry test uses US Letter (612.0 x 792.0), which divides
/// evenly into the 0-500 grid and so never exercises fractional rounding.
/// A4 does not divide evenly, pinning `to_grid`'s rounding behaviour at
/// fractional positions.
#[test]
fn test_loc_tokens_round_correctly_on_odd_page_dimensions() {
    const A4_W: f64 = 595.32;
    const A4_H: f64 = 841.92;
    let out = loc_tokens(&bbox(100.0, 100.0, 400.0, 700.0), (A4_W, A4_H)).unwrap();
    assert_eq!(out, "<loc_84><loc_84><loc_336><loc_441>");
}

#[test]
fn test_render_doctags_emits_loc_tokens_when_geometry_is_available() {
    let mut b = InternalDocumentBuilder::new("pdf");
    b.push_paragraph("Body text.", vec![], Some(1), Some(bbox(61.2, 396.0, 306.0, 792.0)));
    let doc = with_page_dims(b.build(), Some((PAGE_W, PAGE_H)));
    let out = render_doctags(&doc);
    assert_eq!(
        out,
        "<doctag><text><loc_50><loc_0><loc_250><loc_250>Body text.</text>\n</doctag>"
    );
}

/// PPTX fills `y0` with the top edge instead of the bottom, so its geometry
/// must not be normalised as if it were PDF space. It is excluded by never
/// recording page dimensions — this pins that gate.
#[test]
fn test_render_doctags_omits_loc_tokens_without_page_dimensions() {
    let mut b = InternalDocumentBuilder::new("pptx");
    b.push_paragraph("Slide text.", vec![], Some(1), Some(bbox(61.2, 396.0, 306.0, 792.0)));
    let doc = with_page_dims(b.build(), None);
    let out = render_doctags(&doc);
    assert_eq!(out, "<doctag><text>Slide text.</text>\n</doctag>");
    assert!(!out.contains("<loc_"), "got: {}", out);
}

#[test]
fn test_render_doctags_omits_loc_tokens_without_bbox_or_page() {
    let mut b = InternalDocumentBuilder::new("pdf");
    b.push_paragraph("No bbox.", vec![], Some(1), None);
    b.push_paragraph("No page.", vec![], None, Some(bbox(0.0, 0.0, 10.0, 10.0)));
    let doc = with_page_dims(b.build(), Some((PAGE_W, PAGE_H)));
    let out = render_doctags(&doc);
    assert!(!out.contains("<loc_"), "got: {}", out);
}

#[test]
fn test_render_doctags_rejects_degenerate_page_dimensions() {
    let mut b = InternalDocumentBuilder::new("pdf");
    b.push_paragraph("Text.", vec![], Some(1), Some(bbox(0.0, 0.0, 10.0, 10.0)));
    let doc = with_page_dims(b.build(), Some((0.0, PAGE_H)));
    assert!(!render_doctags(&doc).contains("<loc_"));
}

#[test]
fn test_render_doctags_otsl_and_caption_carry_their_own_loc() {
    let mut b = InternalDocumentBuilder::new("pdf");
    let cells = vec![vec!["A".to_string()]];
    let table = b.push_table_from_cells(&cells, Some(1), Some(bbox(61.2, 396.0, 306.0, 792.0)));
    let caption = b.push_paragraph("Table 1.", vec![], Some(1), Some(bbox(61.2, 0.0, 306.0, 396.0)));
    b.push_relationship(caption, RelationshipTarget::Index(table), RelationshipKind::Caption);
    let doc = with_page_dims(b.build(), Some((PAGE_W, PAGE_H)));
    let out = render_doctags(&doc);
    assert!(
        out.contains("<otsl><loc_50><loc_0><loc_250><loc_250><ched>A<nl>"),
        "got: {}",
        out
    );
    assert!(
        out.contains("<caption><loc_50><loc_250><loc_250><loc_500>Table 1.</caption>"),
        "got: {}",
        out
    );
}

/// A document exercising every element kind the renderer handles, so the
/// structural checks run against the full tag surface rather than a slice.
fn kitchen_sink() -> InternalDocument {
    let mut b = InternalDocumentBuilder::new("pdf");
    b.push_title("Doc", Some(1), Some(bbox(61.2, 700.0, 306.0, 780.0)));
    b.push_heading(1, "Section", Some(1), Some(bbox(61.2, 650.0, 306.0, 690.0)));
    b.push_paragraph("Body.", vec![], Some(1), Some(bbox(61.2, 600.0, 306.0, 640.0)));
    b.push_list(true);
    b.push_list_item("First", true, vec![], Some(1), None);
    b.end_list();
    b.push_list_item("Bare", false, vec![], None, None);
    b.push_code("fn main() {}", Some("rust"), Some(1), None);
    b.push_formula("E = mc^2", Some(1), None);
    b.push_raw_block("tex", "\\LaTeX{}", None);
    b.push_admonition("warning", Some("Careful"), None);
    b.push_page_break();

    let cells = vec![
        vec!["H1".to_string(), "H2".to_string()],
        vec!["a".to_string(), String::new()],
        vec!["b".to_string()],
    ];
    let table = b.push_table_from_cells(&cells, Some(1), Some(bbox(61.2, 400.0, 306.0, 500.0)));
    let caption = b.push_paragraph("Table 1.", vec![], Some(1), Some(bbox(61.2, 380.0, 306.0, 395.0)));
    b.push_relationship(caption, RelationshipTarget::Index(table), RelationshipKind::Caption);

    b.push_image(Some("A photo"), image(Some("A photo")), Some(1), None);

    let header = b.push_paragraph("Running header", vec![], Some(1), None);
    b.set_layer(header, ContentLayer::Header);
    let footer = b.push_paragraph("Page 1", vec![], Some(1), None);
    b.set_layer(footer, ContentLayer::Footer);
    b.push_footnote_ref("1", "fn1", None);
    let def = b.push_footnote_definition("A note.", "fn1", Some(1));
    b.set_layer(def, ContentLayer::Footnote);

    with_page_dims(b.build(), Some((PAGE_W, PAGE_H)))
}

#[test]
fn test_rendered_output_is_structurally_valid() {
    let out = render_doctags(&kitchen_sink());
    if let Err(problem) = validate::strict(&out) {
        panic!("invalid DocTags: {}\n{}", problem, out);
    }
}

#[test]
fn test_rendered_output_is_valid_without_geometry() {
    let doc = kitchen_sink();
    let mut stripped = doc.clone();
    stripped.metadata.pages = None;
    let out = render_doctags(&stripped);
    assert!(!out.contains("<loc_"), "got: {}", out);
    if let Err(problem) = validate::strict(&out) {
        panic!("invalid DocTags: {}\n{}", problem, out);
    }
}

/// The validator must accept genuine Docling output, otherwise it is
/// encoding our own idea of the format rather than the format itself.
/// Vocabulary and OTSL checks are omitted: two corpus files predate OTSL
/// and still carry legacy `<table>`/`<tr>`/`<td>` markup.
///
/// `test_documents` is bucket-fetched (see
/// `test_documents/scripts/fetch_corpus.py`) and absent from a bare
/// checkout. Silently doing nothing when it is missing let this test pass
/// in CI while validating zero files, so it now fails loudly by default;
/// set `XBERG_SKIP_TEST_DOCUMENTS=1` to explicitly opt an intentionally
/// unfetched checkout out of this check.
#[test]
fn test_validator_accepts_the_vendored_docling_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_documents");
    let mut checked = 0;
    for dir in ["vendored/docling/txt", "ground_truth/txt"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(".doctags.txt") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            if let Err(problem) = validate::wrapper_and_nesting(content) {
                panic!("{}: {}", path.display(), problem);
            }
            if let Err(problem) = validate::location_tokens(content) {
                panic!("{}: {}", path.display(), problem);
            }
            checked += 1;
        }
    }
    if checked == 0 && std::env::var_os("XBERG_SKIP_TEST_DOCUMENTS").is_some() {
        return;
    }
    assert!(
        checked > 0,
        "no *.doctags.txt fixtures found under {} — run \
         `python3 test_documents/scripts/fetch_corpus.py`, or set \
         XBERG_SKIP_TEST_DOCUMENTS=1 to explicitly skip this check",
        root.display()
    );
}

#[test]
fn test_validator_rejects_malformed_streams() {
    assert!(validate::wrapper_and_nesting("<text>no wrapper</text>").is_err());
    assert!(validate::wrapper_and_nesting("<doctag><text>unclosed</doctag>").is_err());
    assert!(validate::vocabulary("<doctag><nonsense>x</nonsense></doctag>").is_err());
    assert!(validate::location_tokens("<doctag><text><loc_501>x</text></doctag>").is_err());
    // Only three tokens in the group.
    assert!(validate::location_tokens("<doctag><text><loc_1><loc_2><loc_3>x</text></doctag>").is_err());
    // Right edge left of the left edge.
    assert!(validate::location_tokens("<doctag><text><loc_9><loc_2><loc_1><loc_4>x</text></doctag>").is_err());
    assert!(validate::otsl_rows("<doctag><otsl><ched>a<ched>b<nl><fcel>c<nl></otsl></doctag>").is_err());
}

#[test]
fn test_render_doctags_empty_document() {
    let doc = InternalDocumentBuilder::new("test").build();
    assert_eq!(render_doctags(&doc), "<doctag></doctag>");
}

#[test]
fn test_render_doctags_wraps_in_doctag() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("Hello world.", vec![], None, None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag><text>Hello world.</text>\n</doctag>");
}

#[test]
fn test_render_doctags_title_and_headings() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_title("My Document", None, None);
    b.push_heading(1, "Intro", None, None);
    b.push_heading(3, "Detail", None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<title>My Document</title>"), "got: {}", out);
    assert!(
        out.contains("<section_header_level_1>Intro</section_header_level_1>"),
        "got: {}",
        out
    );
    assert!(
        out.contains("<section_header_level_3>Detail</section_header_level_3>"),
        "got: {}",
        out
    );
}

#[test]
fn test_render_doctags_unordered_list_wrapped_and_closed() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_list(false);
    b.push_list_item("Alpha", false, vec![], None, None);
    b.push_list_item("Beta", false, vec![], None, None);
    b.end_list();
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<unordered_list><list_item>Alpha</list_item>\n<list_item>Beta</list_item>\n</unordered_list>"),
        "got: {}",
        out
    );
}

#[test]
fn test_render_doctags_ordered_list_uses_matching_close_tag() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_list(true);
    b.push_list_item("First", true, vec![], None, None);
    b.end_list();
    let out = render_doctags(&b.build());
    assert!(out.contains("<ordered_list><list_item>First"), "got: {}", out);
    assert!(out.contains("</ordered_list>"), "got: {}", out);
    assert!(!out.contains("</unordered_list>"), "got: {}", out);
}

#[test]
fn test_render_doctags_bare_list_items_open_and_close_implicit_wrapper() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_list_item("Alpha", false, vec![], None, None);
    b.push_paragraph("After the list.", vec![], None, None);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<unordered_list><list_item>Alpha</list_item>\n</unordered_list>\n<text>After the list.</text>"),
        "got: {}",
        out
    );
}

/// Two bare list items with no explicit `ListStart`/`ListEnd` between them
/// but differing `ordered` flags must not be silently absorbed into the
/// first item's wrapper — the wrapper must close and reopen as the right
/// kind.
#[test]
fn test_render_doctags_bare_list_items_reopen_wrapper_when_ordering_changes() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_list_item("Alpha", false, vec![], None, None);
    b.push_list_item("One", true, vec![], None, None);
    let out = render_doctags(&b.build());
    assert_eq!(
        out,
        "<doctag><unordered_list><list_item>Alpha</list_item>\n</unordered_list>\n\
         <ordered_list><list_item>One</list_item>\n</ordered_list>\n</doctag>"
    );
}

#[test]
fn test_render_doctags_unterminated_list_closes_at_end() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_list(false);
    b.push_list_item("Alpha", false, vec![], None, None);
    let out = render_doctags(&b.build());
    assert!(out.ends_with("</unordered_list>\n</doctag>"), "got: {}", out);
}

#[test]
fn test_render_doctags_table_as_otsl_with_header_row() {
    let mut b = InternalDocumentBuilder::new("test");
    let cells = vec![
        vec!["Name".to_string(), "Age".to_string()],
        vec!["Alice".to_string(), "30".to_string()],
    ];
    b.push_table_from_cells(&cells, None, None);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<otsl><ched>Name<ched>Age<nl><fcel>Alice<fcel>30<nl></otsl>"),
        "got: {}",
        out
    );
}

#[test]
fn test_render_doctags_table_pads_ragged_rows_with_ecel() {
    let mut b = InternalDocumentBuilder::new("test");
    let cells = vec![vec!["A".to_string(), "B".to_string()], vec!["only".to_string()]];
    b.push_table_from_cells(&cells, None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<fcel>only<ecel><nl>"), "got: {}", out);
}

#[test]
fn test_render_doctags_table_empty_cell_becomes_ecel() {
    let mut b = InternalDocumentBuilder::new("test");
    let cells = vec![
        vec!["A".to_string(), "B".to_string()],
        vec!["".to_string(), "value".to_string()],
    ];
    b.push_table_from_cells(&cells, None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<ecel><fcel>value<nl>"), "got: {}", out);
}

#[test]
fn test_render_doctags_empty_table_emits_nothing() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_table_from_cells(&[], None, None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag></doctag>");
}

/// Regression test for a shipped bug: `push_otsl` drops empty/degenerate
/// tables silently, but the caption-nesting pass upstream had already
/// decided the caption would render *inside* the (never-emitted) `<otsl>`
/// and so skipped rendering it on its own. The net effect was that a
/// caption on an empty table vanished with no trace. This mirrors the
/// parser's own stated behaviour: a caption whose target was dropped
/// still carries text, so it stays as an ordinary text element.
#[test]
fn test_render_doctags_empty_table_caption_survives_as_text() {
    let mut b = InternalDocumentBuilder::new("test");
    let table = b.push_table_from_cells(&[], None, None);
    let caption = b.push_paragraph("Table 1. Empty.", vec![], None, None);
    b.push_relationship(caption, RelationshipTarget::Index(table), RelationshipKind::Caption);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag><text>Table 1. Empty.</text>\n</doctag>");
}

#[test]
fn test_render_doctags_code_carries_language_token() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_code("fn main() {}", Some("rust"), None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<code><_rust_>fn main() {}</code>"), "got: {}", out);
}

#[test]
fn test_render_doctags_code_without_language_uses_unknown_token() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_code("echo hi", None, None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<code><_unknown_>echo hi</code>"), "got: {}", out);
}

#[test]
fn test_render_doctags_code_is_flattened_to_one_line() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_code("line one\nline two", None, None, None);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<code><_unknown_>line one line two</code>"),
        "got: {}",
        out
    );
    assert_eq!(out.lines().count(), 2, "got: {}", out);
}

#[test]
fn test_render_doctags_formula() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_formula("E = mc^2", None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<formula>E = mc^2</formula>"), "got: {}", out);
}

#[test]
fn test_render_doctags_page_break() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("Before", vec![], None, None);
    b.push_page_break();
    b.push_paragraph("After", vec![], None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("</text>\n<page_break>\n<text>After"), "got: {}", out);
}

#[test]
fn test_render_doctags_image_becomes_picture() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_image(Some("A nice photo"), image(Some("A nice photo")), None, None);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<picture><caption>A nice photo</caption></picture>"),
        "got: {}",
        out
    );
}

#[test]
fn test_render_doctags_image_without_description_has_no_caption() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_image(None, image(None), None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<picture></picture>"), "got: {}", out);
    assert!(!out.contains("<caption>"), "got: {}", out);
}

#[test]
fn test_render_doctags_content_layer_selects_tag() {
    let mut b = InternalDocumentBuilder::new("test");
    let header = b.push_paragraph("Running header", vec![], None, None);
    b.set_layer(header, ContentLayer::Header);
    let footer = b.push_paragraph("Page 3", vec![], None, None);
    b.set_layer(footer, ContentLayer::Footer);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<page_header>Running header</page_header>"),
        "got: {}",
        out
    );
    assert!(out.contains("<page_footer>Page 3</page_footer>"), "got: {}", out);
}

#[test]
fn test_render_doctags_footnote_definition() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("Main text", vec![], None, None);
    b.push_footnote_ref("1", "fn1", None);
    let def = b.push_footnote_definition("A note.", "fn1", None);
    b.set_layer(def, ContentLayer::Footnote);
    let out = render_doctags(&b.build());
    assert!(out.contains("<footnote>A note.</footnote>"), "got: {}", out);
}

#[test]
fn test_render_doctags_footnote_ref_is_not_emitted() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_footnote_ref("1", "fn1", None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag></doctag>");
}

#[test]
fn test_render_doctags_text_is_not_escaped() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("results & performance for a < b", vec![], None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("results & performance for a < b"), "got: {}", out);
    assert!(!out.contains("&amp;"), "got: {}", out);
}

#[test]
fn test_render_doctags_paragraph_newlines_collapse_to_one_line() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("wrapped\nacross\nlines", vec![], None, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<text>wrapped across lines</text>"), "got: {}", out);
    assert_eq!(out.lines().count(), 2, "got: {}", out);
}

#[test]
fn test_render_doctags_empty_paragraph_is_skipped() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_paragraph("", vec![], None, None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag></doctag>");
}

#[test]
fn test_render_doctags_quote_and_group_markers_are_transparent() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_quote_start();
    b.push_paragraph("Quoted text.", vec![], None, None);
    b.push_quote_end();
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag><text>Quoted text.</text>\n</doctag>");
}

#[test]
fn test_render_doctags_metadata_block_emits_one_text_per_entry() {
    let mut b = InternalDocumentBuilder::new("test");
    let entries = vec![
        ("Author".to_string(), "Alice".to_string()),
        ("Date".to_string(), "2026".to_string()),
    ];
    b.push_metadata_block(&entries, None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<text>Author: Alice</text>"), "got: {}", out);
    assert!(out.contains("<text>Date: 2026</text>"), "got: {}", out);
}

/// Regression test for a shipped bug: the old implementation pushed the
/// label (title-or-kind) *and* `elem.text` as two separate `<text>`
/// elements. Since `push_admonition` sets `elem.text` to
/// `title.unwrap_or(kind)`, those two strings are always identical, so
/// every admonition rendered its label twice. Asserting the exact,
/// complete output (rather than `contains`) is what makes this fail
/// against the old code — `contains("<text>Be careful</text>")` was true
/// whether the line appeared once or twice.
#[test]
fn test_render_doctags_admonition_with_title_renders_once() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_admonition("warning", Some("Be careful"), None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag><text>Be careful</text>\n</doctag>");
}

#[test]
fn test_render_doctags_admonition_without_title_uses_kind_as_label_once() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_admonition("note", None, None);
    let out = render_doctags(&b.build());
    assert_eq!(out, "<doctag><text>note</text>\n</doctag>");
}

#[test]
fn test_render_doctags_raw_block_becomes_code() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_raw_block("tex", "\\LaTeX{}", None);
    let out = render_doctags(&b.build());
    assert!(out.contains("<code><_unknown_>\\LaTeX{}</code>"), "got: {}", out);
}

#[test]
fn test_render_doctags_table_caption_nests_and_is_not_emitted_twice() {
    let mut b = InternalDocumentBuilder::new("test");
    let cells = vec![vec!["A".to_string()]];
    let table = b.push_table_from_cells(&cells, None, None);
    let caption = b.push_paragraph("Table 1. Results.", vec![], None, None);
    b.push_relationship(caption, RelationshipTarget::Index(table), RelationshipKind::Caption);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<caption>Table 1. Results.</caption></otsl>"),
        "got: {}",
        out
    );
    assert!(!out.contains("<text>Table 1. Results.</text>"), "got: {}", out);
}

#[test]
fn test_render_doctags_picture_caption_relationship_wins_over_description() {
    let mut b = InternalDocumentBuilder::new("test");
    let picture = b.push_image(Some("alt text"), image(Some("alt text")), None, None);
    let caption = b.push_paragraph("Figure 1. A diagram.", vec![], None, None);
    b.push_relationship(caption, RelationshipTarget::Index(picture), RelationshipKind::Caption);
    let out = render_doctags(&b.build());
    assert!(
        out.contains("<picture><caption>Figure 1. A diagram.</caption></picture>"),
        "got: {}",
        out
    );
    assert!(!out.contains("alt text"), "got: {}", out);
}

/// `"doctags"` resolves to the first-class `OutputFormat::DocTags` variant.
///
/// This test previously pinned `Custom("doctags")`, from when the renderer was
/// reachable only through the registry fallback that `FromStr` gives every
/// unrecognised string. Promoting DocTags to a real variant changed what these
/// two entry points return, so the expectation is updated rather than the code:
/// `FromStr` is the API-handler path and serde is the path the language bindings
/// take (e.g. Python's `OutputFormat("doctags")`), and both must now agree on the
/// variant. The `Custom` route is asserted separately below because it stays
/// live for anyone who constructed it explicitly.
#[test]
fn test_doctags_string_resolves_to_the_first_class_variant() {
    use crate::core::config::OutputFormat;
    use std::str::FromStr;

    assert_eq!(OutputFormat::from_str("doctags").unwrap(), OutputFormat::DocTags);
    assert_eq!(
        serde_json::from_str::<OutputFormat>("\"doctags\"").unwrap(),
        OutputFormat::DocTags
    );

    const EXPECTED: &str = "<doctag><title>Doc</title>\n<text>Body text.</text>\n</doctag>";

    let render_with = |format: OutputFormat| {
        let mut b = InternalDocumentBuilder::new("test");
        b.push_title("Doc", None, None);
        b.push_paragraph("Body text.", vec![], None, None);
        crate::extraction::derive::derive_extraction_result(b.build(), false, format).formatted_content
    };

    assert_eq!(render_with(OutputFormat::DocTags).as_deref(), Some(EXPECTED));
    // Backward compatibility: an explicitly constructed `Custom("doctags")` still
    // routes through the renderer registry and produces identical output. ~keep
    assert_eq!(
        render_with(OutputFormat::Custom("doctags".to_string())).as_deref(),
        Some(EXPECTED)
    );
}

/// LaTeX `figure` with placeholder injection attaches a caption to a plain
/// paragraph. `<text>` cannot nest a `<caption>`, so the caption must still
/// render as its own element instead of being dropped.
#[test]
fn test_render_doctags_caption_on_paragraph_target_is_not_dropped() {
    let mut b = InternalDocumentBuilder::new("test");
    let placeholder = b.push_paragraph("[image: diagram.png]", vec![], None, None);
    let caption = b.push_paragraph("Figure 1. A diagram.", vec![], None, None);
    b.push_relationship(
        caption,
        RelationshipTarget::Index(placeholder),
        RelationshipKind::Caption,
    );
    let out = render_doctags(&b.build());
    assert!(out.contains("<text>Figure 1. A diagram.</text>"), "got: {}", out);
    assert!(out.contains("<text>[image: diagram.png]</text>"), "got: {}", out);
}

#[test]
fn test_render_doctags_every_element_is_on_its_own_line() {
    let mut b = InternalDocumentBuilder::new("test");
    b.push_title("Doc", None, None);
    b.push_paragraph("One", vec![], None, None);
    b.push_paragraph("Two", vec![], None, None);
    let out = render_doctags(&b.build());
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "got: {}", out);
    assert!(lines[0].starts_with("<doctag><title>"), "got: {}", out);
    assert_eq!(lines[3], "</doctag>", "got: {}", out);
}
