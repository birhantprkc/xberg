//! Parse Docling DocTags into an `InternalDocument`.
//!
//! The inverse of [`crate::rendering::doctags`]. Together they let xberg consume
//! output from Docling and from the SmolDocling / Granite-Docling models, and
//! hand its own documents back in the same format.
//!
//! Two properties of the format drive the design:
//!
//! - **There is no escaping.** Real Docling output carries literal `<` inside
//!   prose: a caption in the vendored `2203.01017v2` corpus discusses
//!   `' < td > '`. Scanning for the next `>` would swallow the `</caption>`
//!   that follows it. So only *recognised* tag names are treated as tags and
//!   everything else is content, which is how Docling's own tokenizer behaves.
//! - **Location tokens are page-relative.** `<loc_*>` values are normalised
//!   onto a 0–500 grid, and the original page size is not recoverable from the
//!   stream. Pages are therefore reconstructed as `LOC_GRID` squares, which
//!   makes re-emitting a parsed document reproduce the original tokens exactly.
//!
//! OTSL merge tokens are expanded into the flat cell grid `Table` uses: `lcel`
//! repeats the cell to its left, `ucel` the cell above, `xcel` either. The
//! merge itself is not preserved, because `Table::cells` has nowhere to put it.

use crate::types::document_structure::{ContentLayer, RelationshipKind};
use crate::types::extraction::{BoundingBox, ExtractedImage};
use crate::types::internal::{InternalDocument, RelationshipTarget};
use crate::types::internal_builder::InternalDocumentBuilder;

/// DocTags normalises bounding boxes onto a fixed square grid of this size.
pub(crate) const LOC_GRID: f64 = 500.0;

/// Cell tokens that occupy one OTSL grid position.
pub(crate) const OTSL_CELLS: &[&str] = &["fcel", "ched", "ecel", "lcel", "ucel", "xcel", "rhed"];

/// Tokens that stand alone rather than wrapping content.
pub(crate) const STANDALONE: &[&str] = &["nl", "page_break"];

/// Tags that wrap content and must be closed.
///
/// `checkbox_*` wrap their label in real Docling output, e.g.
/// `<checkbox_unselected><loc_…>بلی</checkbox_unselected>`.
pub(crate) const PAIRED: &[&str] = &[
    "doctag",
    "checkbox_selected",
    "checkbox_unselected",
    "text",
    "title",
    "page_header",
    "page_footer",
    "footnote",
    "caption",
    "code",
    "formula",
    "otsl",
    "picture",
    "list_item",
    "ordered_list",
    "unordered_list",
];

/// Whether `name` is a token that stands alone.
pub(crate) fn is_standalone(name: &str) -> bool {
    STANDALONE.contains(&name)
        || OTSL_CELLS.contains(&name)
        || name.starts_with("loc_")
        // Code language tokens, e.g. `<_rust_>` and `<_unknown_>`.
        || (name.len() > 1 && name.starts_with('_') && name.ends_with('_'))
}

/// Whether `name` is a tag that wraps content.
pub(crate) fn is_paired(name: &str) -> bool {
    PAIRED.contains(&name) || name.starts_with("section_header_level_")
}

/// A lexical unit of a DocTags stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Token<'a> {
    Open(&'a str),
    Close(&'a str),
    Text(&'a str),
}

/// Split a DocTags stream into tags and content.
///
/// Anything that is not a recognised tag stays content, including stray `<`.
pub(crate) fn tokenize(input: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut cursor = 0;
    let mut text_start = 0;

    while cursor < bytes.len() {
        if bytes[cursor] != b'<' {
            cursor += 1;
            continue;
        }
        let Some(offset) = input[cursor..].find('>') else { break };
        let end = cursor + offset;
        let raw = &input[cursor + 1..end];
        let name = raw.strip_prefix('/').unwrap_or(raw);

        if !is_standalone(name) && !is_paired(name) {
            cursor += 1;
            continue;
        }
        if text_start < cursor {
            out.push(Token::Text(&input[text_start..cursor]));
        }
        out.push(if raw.starts_with('/') {
            Token::Close(name)
        } else {
            Token::Open(name)
        });
        cursor = end + 1;
        text_start = cursor;
    }

    if text_start < input.len() {
        out.push(Token::Text(&input[text_start..]));
    }
    out
}

/// Index of the `Close` matching the `Open` at `open_at`, or the slice end.
///
/// Tracks a stack of open tag *names*, not just nesting depth: a `Close`
/// only pops when it names the innermost open tag. Without that check, a
/// stray `</text>` sitting inside an unrelated `<otsl>...</otsl>` region
/// would still decrement a name-blind depth counter and could close the
/// wrong element early, truncating or misattributing its content.
fn matching_close(tokens: &[Token<'_>], open_at: usize) -> usize {
    let Some(Token::Open(open_name)) = tokens.get(open_at) else {
        return tokens.len();
    };
    let mut stack = vec![*open_name];
    for (index, token) in tokens.iter().enumerate().skip(open_at + 1) {
        match token {
            Token::Open(name) if is_paired(name) => stack.push(*name),
            // A close that does not name the innermost open tag belongs to some
            // other element, so it falls through to the catch-all and is ignored
            // rather than decrementing this element's nesting.
            Token::Close(name) if is_paired(name) && stack.last() == Some(name) => {
                stack.pop();
                if stack.is_empty() {
                    return index;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

/// Read a leading group of four location tokens.
///
/// Returns the box and how many tokens it consumed. DocTags orders them left,
/// top, right, bottom on a top-left origin; `BoundingBox` is PDF space with a
/// bottom-left origin, so the vertical axis is flipped back here.
fn take_location(tokens: &[Token<'_>]) -> (Option<BoundingBox>, usize) {
    let mut values = Vec::with_capacity(4);
    for token in tokens.iter().take(4) {
        let Token::Open(name) = token else { break };
        let Some(raw) = name.strip_prefix("loc_") else { break };
        let Ok(value) = raw.parse::<f64>() else { break };
        // A hostile or corrupt `<loc_nan>` / `<loc_-1>` token must not
        // silently become a plausible-looking box, so reject rather than
        // clamp; this reuses the `values.len() != 4` absent-location path
        // below and mirrors the renderer's `is_finite` guard in
        // `rendering::doctags::loc_tokens`. ~keep
        if !value.is_finite() || !(0.0..=LOC_GRID).contains(&value) {
            break;
        }
        values.push(value);
    }
    if values.len() != 4 {
        return (None, 0);
    }
    (
        Some(BoundingBox {
            x0: values[0],
            y0: LOC_GRID - values[3],
            x1: values[2],
            y1: LOC_GRID - values[1],
        }),
        4,
    )
}

/// Concatenate the content of a token slice, ignoring nested tags.
fn text_of(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for token in tokens {
        if let Token::Text(text) = token {
            out.push_str(text);
        }
    }
    out.trim().to_string()
}

/// Locate a nested `<caption>` within an element's inner tokens.
fn find_caption(tokens: &[Token<'_>]) -> Option<(usize, usize)> {
    let start = tokens.iter().position(|t| matches!(t, Token::Open("caption")))?;
    let end = matching_close(tokens, start);
    Some((start, end))
}

/// Expand an OTSL cell stream into the flat grid `Table` stores.
///
/// Merge tokens repeat the content they continue, since the flat grid cannot
/// record a span. Rows are padded so the grid stays rectangular.
fn parse_otsl_cells(tokens: &[Token<'_>]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();

    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            Token::Open("nl") => {
                rows.push(std::mem::take(&mut row));
                index += 1;
            }
            Token::Open(name) if OTSL_CELLS.contains(&name) => {
                let content = match tokens.get(index + 1) {
                    Some(Token::Text(text)) => text.trim().to_string(),
                    _ => String::new(),
                };
                let value = match name {
                    "lcel" => row.last().cloned().unwrap_or_default(),
                    "ucel" => rows
                        .last()
                        .and_then(|previous| previous.get(row.len()))
                        .cloned()
                        .unwrap_or_default(),
                    "xcel" => row
                        .last()
                        .cloned()
                        .or_else(|| rows.last().and_then(|previous| previous.get(row.len())).cloned())
                        .unwrap_or_default(),
                    _ => content,
                };
                row.push(value);
                index += 1;
            }
            // A caption ends the cell stream.
            Token::Open("caption") => break,
            _ => index += 1,
        }
    }

    if !row.is_empty() {
        rows.push(row);
    }

    let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(width, String::new());
    }
    rows
}

/// Parse a DocTags stream into an `InternalDocument`.
pub(crate) fn parse_doctags(input: &str) -> InternalDocument {
    let tokens = tokenize(input);
    let mut builder = InternalDocumentBuilder::new("doctags");
    let mut page: u32 = 1;
    let mut pages_seen: u32 = 1;
    let mut index = 0;
    let mut stray_text_warned = false;

    while index < tokens.len() {
        let token = tokens[index];
        let Token::Open(name) = token else {
            match token {
                Token::Close(name) if name == "ordered_list" || name == "unordered_list" => {
                    builder.end_list();
                }
                Token::Text(text) => {
                    push_stray_text(&mut builder, text, page, &mut stray_text_warned);
                }
                _ => {}
            }
            index += 1;
            continue;
        };

        match name {
            "doctag" => index += 1,
            "page_break" => {
                builder.push_page_break();
                page += 1;
                pages_seen = pages_seen.max(page);
                index += 1;
            }
            "ordered_list" | "unordered_list" => {
                builder.push_list(name == "ordered_list");
                index += 1;
            }
            _ if is_paired(name) => {
                let close = matching_close(&tokens, index);
                let inner = &tokens[index + 1..close.min(tokens.len())];
                push_element(&mut builder, name, inner, page);
                index = close + 1;
            }
            _ => index += 1,
        }
    }

    let mut doc = builder.build();
    doc.metadata.pages = Some(reconstructed_pages(pages_seen));
    doc.mime_type = crate::core::mime::DOCTAGS_MIME_TYPE.to_string();
    doc
}

/// Rebuild page metadata as `LOC_GRID` squares.
///
/// The true page size is not in the stream, and using the grid itself as the
/// page means re-emitting reproduces the original `<loc_*>` values exactly.
fn reconstructed_pages(count: u32) -> crate::types::PageStructure {
    crate::types::PageStructure {
        total_count: count,
        unit_type: crate::types::PageUnitType::Page,
        boundaries: None,
        pages: Some(
            (1..=count)
                .map(|number| crate::types::PageInfo {
                    number,
                    title: None,
                    dimensions: Some((LOC_GRID, LOC_GRID).into()),
                    image_count: None,
                    table_count: None,
                    hidden: None,
                    is_blank: None,
                    has_vector_graphics: false,
                })
                .collect(),
        ),
    }
}

/// Source tag used on `ProcessingWarning`s raised while parsing DocTags.
const DOCTAGS_WARNING_SOURCE: &str = "doctags";

/// Keep text found outside any recognised tag rather than discarding it.
///
/// A truncated or corrupted stream — or a plain-text file misrouted to this
/// extractor — may carry no recognised wrapper tags at all; dropping that
/// text would turn a partial or malformed input into an empty, falsely
/// "successful" extraction. It is kept as an ordinary paragraph instead,
/// following the same philosophy as the orphaned-caption path in
/// `push_element`: text that survives is content, even without structure
/// around it. Whitespace-only runs between recognised tags (e.g. the `\n`
/// DocTags emits between elements) are not content and are ignored.
fn push_stray_text(builder: &mut InternalDocumentBuilder, text: &str, page: u32, warned: &mut bool) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    builder.push_paragraph(trimmed, Vec::new(), Some(page), None);
    if !*warned {
        builder.add_warning(crate::core::diagnostics::warning(
            DOCTAGS_WARNING_SOURCE,
            "Text appeared outside any recognised DocTags tag; it was kept as a plain \
             paragraph, but the page, table or list structure it belonged to could not \
             be reconstructed",
        ));
        *warned = true;
    }
}

/// Push the element body itself (everything but the caption handling in [`push_element`]),
/// dispatching on tag name to the right `InternalDocumentBuilder` push method.
fn push_element_body(
    builder: &mut InternalDocumentBuilder,
    name: &str,
    content: &[Token<'_>],
    page: Option<u32>,
    bbox: Option<BoundingBox>,
) -> Option<u32> {
    match name {
        "otsl" => {
            let cells = parse_otsl_cells(content);
            // Docling emits table regions it found no cells in. There is no
            // table there, and inventing an empty one would not survive a
            // re-emit, so it is dropped.
            if cells.is_empty() {
                None
            } else {
                Some(builder.push_table_from_cells(&cells, page, bbox))
            }
        }
        "picture" => Some(builder.push_image(None, ExtractedImage::default(), page, bbox)),
        "code" => {
            let language = content.iter().find_map(|token| match token {
                Token::Open(name) if name.starts_with('_') && name.ends_with('_') => {
                    let trimmed = name.trim_matches('_');
                    (!trimmed.is_empty() && trimmed != "unknown").then_some(trimmed)
                }
                _ => None,
            });
            Some(builder.push_code(&text_of(content), language, page, bbox))
        }
        "formula" => Some(builder.push_formula(&text_of(content), page, bbox)),
        "title" => Some(builder.push_title(&text_of(content), page, bbox)),
        "list_item" => Some(builder.push_list_item(&text_of(content), false, Vec::new(), page, bbox)),
        "footnote" => {
            let index = builder.push_footnote_definition(&text_of(content), "", page);
            builder.set_layer(index, ContentLayer::Footnote);
            Some(index)
        }
        "page_header" | "page_footer" => {
            let index = builder.push_paragraph(&text_of(content), Vec::new(), page, bbox);
            builder.set_layer(
                index,
                if name == "page_header" {
                    ContentLayer::Header
                } else {
                    ContentLayer::Footer
                },
            );
            Some(index)
        }
        "checkbox_selected" | "checkbox_unselected" => {
            let marker = if name == "checkbox_selected" { "[x]" } else { "[ ]" };
            let text = text_of(content);
            let text = if text.is_empty() {
                marker.to_string()
            } else {
                format!("{} {}", marker, text)
            };
            Some(builder.push_paragraph(&text, Vec::new(), page, bbox))
        }
        _ if name.starts_with("section_header_level_") => {
            let level = name
                .trim_start_matches("section_header_level_")
                .parse::<u8>()
                .unwrap_or(1)
                .clamp(1, 6);
            Some(builder.push_heading(level, &text_of(content), page, bbox))
        }
        // `text` and anything else that wraps prose.
        _ => Some(builder.push_paragraph(&text_of(content), Vec::new(), page, bbox)),
    }
}

/// Push one parsed element, plus its caption when it has one.
fn push_element(builder: &mut InternalDocumentBuilder, name: &str, inner: &[Token<'_>], page: u32) {
    let (bbox, consumed) = take_location(inner);
    let body = &inner[consumed..];
    let page = Some(page);

    let caption = find_caption(body).map(|(start, end)| {
        let caption_inner = &body[start + 1..end.min(body.len())];
        let (caption_bbox, caption_consumed) = take_location(caption_inner);
        (text_of(&caption_inner[caption_consumed..]), caption_bbox)
    });
    let content_end = find_caption(body).map(|(start, _)| start).unwrap_or(body.len());
    let content = &body[..content_end];

    let element = push_element_body(builder, name, content, page, bbox);

    if let Some((caption_text, caption_bbox)) = caption
        && !caption_text.is_empty()
    {
        let caption_index = builder.push_paragraph(&caption_text, Vec::new(), page, caption_bbox);
        // A caption whose target was dropped still carries text, so it stays as
        // an ordinary paragraph rather than being discarded with the target.
        if let Some(element) = element {
            builder.push_relationship(
                caption_index,
                RelationshipTarget::Index(element),
                RelationshipKind::Caption,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::render_doctags;
    use crate::types::internal::ElementKind;

    #[test]
    fn test_tokenize_treats_unknown_angle_runs_as_text() {
        // The vendored corpus discusses `< td >` inside a caption. Treating it
        // as a tag would swallow the `</caption>` that follows.
        let tokens = tokenize("<doctag><caption>cells (' < td > ', ' < ')</caption></doctag>");
        assert!(tokens.contains(&Token::Close("caption")), "{:?}", tokens);
        assert!(tokens.contains(&Token::Close("doctag")), "{:?}", tokens);
    }

    #[test]
    fn test_parse_text_and_headings() {
        let doc = parse_doctags(
            "<doctag><title>Doc</title>\n<section_header_level_2>Sec</section_header_level_2>\n<text>Body.</text>\n</doctag>",
        );
        assert_eq!(doc.elements.len(), 3);
        assert_eq!(doc.elements[0].kind, ElementKind::Title);
        assert_eq!(doc.elements[1].kind, ElementKind::Heading { level: 2 });
        assert_eq!(doc.elements[2].text, "Body.");
    }

    #[test]
    fn test_parse_content_layers() {
        let doc = parse_doctags(
            "<doctag><page_header>Head</page_header>\n<page_footer>Foot</page_footer>\n<footnote>Note</footnote>\n</doctag>",
        );
        assert_eq!(doc.elements[0].layer, ContentLayer::Header);
        assert_eq!(doc.elements[1].layer, ContentLayer::Footer);
        assert_eq!(doc.elements[2].layer, ContentLayer::Footnote);
    }

    #[test]
    fn test_parse_location_tokens_into_bbox() {
        let doc = parse_doctags("<doctag><text><loc_50><loc_0><loc_250><loc_250>Body.</text>\n</doctag>");
        let bbox = doc.elements[0].bbox.expect("bbox");
        assert_eq!(bbox.x0, 50.0);
        assert_eq!(bbox.x1, 250.0);
        // Top of the grid is the top of the page, so y1 is the full height.
        assert_eq!(bbox.y1, 500.0);
        assert_eq!(bbox.y0, 250.0);
    }

    #[test]
    fn test_parse_otsl_expands_merge_tokens() {
        let doc = parse_doctags("<doctag><otsl><ched>A<ched>B<nl><fcel>x<lcel><nl><fcel>y<ucel><nl></otsl>\n</doctag>");
        let table = &doc.tables[0];
        assert_eq!(table.cells[0], vec!["A".to_string(), "B".to_string()]);
        // `lcel` continues the cell to its left.
        assert_eq!(table.cells[1], vec!["x".to_string(), "x".to_string()]);
        // `ucel` continues the cell above.
        assert_eq!(table.cells[2], vec!["y".to_string(), "x".to_string()]);
    }

    #[test]
    fn test_parse_otsl_pads_ragged_rows() {
        let doc = parse_doctags("<doctag><otsl><ched>A<ched>B<nl><fcel>only<nl></otsl>\n</doctag>");
        let table = &doc.tables[0];
        assert_eq!(table.cells[1].len(), 2);
        assert_eq!(table.cells[1][1], "");
    }

    #[test]
    fn test_parse_caption_attaches_relationship() {
        let doc = parse_doctags("<doctag><otsl><ched>A<nl><caption>Table 1.</caption></otsl>\n</doctag>");
        let caption = doc
            .elements
            .iter()
            .position(|e| e.text == "Table 1.")
            .expect("caption element");
        assert!(
            doc.relationships
                .iter()
                .any(|r| r.kind == RelationshipKind::Caption && r.source == caption as u32),
            "caption relationship missing: {:?}",
            doc.relationships
        );
    }

    #[test]
    fn test_parse_lists_and_page_breaks() {
        let doc = parse_doctags(
            "<doctag><ordered_list><list_item>One</list_item>\n</ordered_list>\n<page_break>\n<text>Next</text>\n</doctag>",
        );
        assert!(
            doc.elements
                .iter()
                .any(|e| e.kind == ElementKind::ListStart { ordered: true })
        );
        assert!(doc.elements.iter().any(|e| e.kind == ElementKind::ListEnd));
        assert!(doc.elements.iter().any(|e| e.kind == ElementKind::PageBreak));
        // Content after a page break belongs to the next page.
        let next = doc.elements.iter().find(|e| e.text == "Next").expect("element");
        assert_eq!(next.page, Some(2));
    }

    #[test]
    fn test_parse_code_language_token() {
        let doc = parse_doctags("<doctag><code><_rust_>fn main() {}</code>\n</doctag>");
        assert_eq!(doc.elements[0].kind, ElementKind::Code);
        assert_eq!(doc.elements[0].text, "fn main() {}");
        let language = doc.elements[0]
            .attributes
            .as_ref()
            .and_then(|a| a.get("language"))
            .map(String::as_str);
        assert_eq!(language, Some("rust"));
    }

    #[test]
    fn test_parse_unknown_language_token_is_not_recorded() {
        let doc = parse_doctags("<doctag><code><_unknown_>echo hi</code>\n</doctag>");
        assert_eq!(doc.elements[0].text, "echo hi");
        let language = doc.elements[0].attributes.as_ref().and_then(|a| a.get("language"));
        assert_eq!(language, None);
    }

    /// The property that matters for interop: emitting a parsed document
    /// reproduces the stream it came from.
    /// Parse every vendored Docling file. This is the interoperability check:
    /// the parser has to cope with real model output, not just what we emit.
    #[test]
    fn test_parses_the_vendored_docling_corpus() {
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
                if content.trim().is_empty() {
                    continue;
                }
                let doc = parse_doctags(&content);
                assert!(!doc.elements.is_empty(), "{} parsed to nothing", path.display());
                // Re-emitting must stay parseable, which catches a parser that
                // silently drops structure it did not understand.
                let reemitted = render_doctags(&doc);
                let reparsed = parse_doctags(&reemitted);
                assert_eq!(
                    reparsed.elements.len(),
                    doc.elements.len(),
                    "{} lost elements on the second pass",
                    path.display()
                );
                checked += 1;
            }
        }
        // `test_documents` is bucket-fetched, not vendored, so a fresh checkout
        // has none of these files; CI fetches it before running tests. Fail
        // unless the opt-out is set explicitly, so this — the only test
        // validating against real Docling output rather than our own
        // assumptions — cannot pass while silently checking nothing.
        assert!(
            checked > 0 || std::env::var_os("XBERG_SKIP_TEST_DOCUMENTS").is_some(),
            "no vendored DocTags fixtures were found under {}; run \
             `python3 test_documents/scripts/fetch_corpus.py` to populate test_documents, \
             or set XBERG_SKIP_TEST_DOCUMENTS=1 to skip this check locally",
            root.display()
        );
    }

    #[test]
    fn test_round_trip_is_stable() {
        let original = concat!(
            "<doctag><title><loc_10><loc_20><loc_300><loc_40>Doc</title>\n",
            "<section_header_level_1><loc_10><loc_50><loc_300><loc_60>Section</section_header_level_1>\n",
            "<text><loc_10><loc_70><loc_300><loc_120>Body text.</text>\n",
            "<unordered_list><list_item>Alpha</list_item>\n",
            "<list_item>Beta</list_item>\n",
            "</unordered_list>\n",
            "<page_break>\n",
            "<otsl><loc_10><loc_200><loc_300><loc_260><ched>A<ched>B<nl><fcel>c<fcel>d<nl>",
            "<caption><loc_10><loc_270><loc_300><loc_280>Table 1.</caption></otsl>\n",
            "<formula><loc_10><loc_300><loc_300><loc_320>E = mc^2</formula>\n",
            "<page_footer><loc_10><loc_480><loc_300><loc_490>Page 2</page_footer>\n",
            "</doctag>"
        );

        let reemitted = render_doctags(&parse_doctags(original));
        assert_eq!(reemitted, original);
    }

    #[test]
    fn test_round_trip_of_renderer_output_is_idempotent() {
        let first = concat!(
            "<doctag><text><loc_10><loc_20><loc_300><loc_40>One</text>\n",
            "<picture><loc_10><loc_50><loc_300><loc_90><caption>A figure.</caption></picture>\n",
            "</doctag>"
        );
        let once = render_doctags(&parse_doctags(first));
        let twice = render_doctags(&parse_doctags(&once));
        assert_eq!(once, twice, "parse/render is not idempotent");
    }

    #[test]
    fn should_drop_bbox_when_location_token_is_nan() {
        let doc = parse_doctags("<doctag><text><loc_nan><loc_0><loc_1><loc_1>x</text>\n</doctag>");
        assert_eq!(doc.elements.len(), 1);
        assert_eq!(doc.elements[0].bbox, None);
        assert_eq!(doc.elements[0].text, "x");
    }

    #[test]
    fn should_drop_bbox_when_location_token_is_infinite() {
        let doc = parse_doctags("<doctag><text><loc_inf><loc_0><loc_1><loc_1>x</text>\n</doctag>");
        assert_eq!(doc.elements[0].bbox, None);
        assert_eq!(doc.elements[0].text, "x");
    }

    #[test]
    fn should_drop_bbox_when_location_token_is_negative() {
        let doc = parse_doctags("<doctag><text><loc_-1><loc_0><loc_1><loc_1>x</text>\n</doctag>");
        assert_eq!(doc.elements[0].bbox, None);
        assert_eq!(doc.elements[0].text, "x");
    }

    #[test]
    fn should_drop_bbox_when_location_token_exceeds_grid() {
        let doc = parse_doctags("<doctag><text><loc_501><loc_0><loc_1><loc_1>x</text>\n</doctag>");
        assert_eq!(doc.elements[0].bbox, None);
        assert_eq!(doc.elements[0].text, "x");
    }

    #[test]
    fn should_keep_finite_in_range_bbox_at_the_grid_boundary() {
        // 500 is the top of the grid (`LOC_GRID`) and must stay valid, unlike
        // the out-of-range rejection above.
        let doc = parse_doctags("<doctag><text><loc_0><loc_0><loc_500><loc_500>x</text>\n</doctag>");
        let bbox = doc.elements[0].bbox.expect("bbox should survive boundary values");
        assert_eq!(bbox.x0, 0.0);
        assert_eq!(bbox.x1, 500.0);
        assert_eq!(bbox.y0, 0.0);
        assert_eq!(bbox.y1, 500.0);
    }

    #[test]
    fn should_capture_text_as_paragraph_when_no_tags_are_recognised() {
        let doc = parse_doctags("This is a corrupted stream with no doctags markup at all.");
        assert_eq!(doc.elements.len(), 1);
        assert_eq!(
            doc.elements[0].text,
            "This is a corrupted stream with no doctags markup at all."
        );
        assert_eq!(doc.processing_warnings.len(), 1);
        assert_eq!(doc.processing_warnings[0].source, DOCTAGS_WARNING_SOURCE);
    }

    #[test]
    fn should_ignore_whitespace_only_text_between_recognised_tags() {
        // The `\n` DocTags emits between elements must not become a stray
        // paragraph or trigger the unwrapped-text warning.
        let doc = parse_doctags("<doctag><title>Doc</title>\n<text>Body.</text>\n</doctag>");
        assert_eq!(doc.elements.len(), 2);
        assert!(doc.processing_warnings.is_empty());
    }

    #[test]
    fn should_warn_once_when_multiple_stray_text_runs_occur() {
        let doc = parse_doctags("stray one <text>Body.</text> stray two");
        assert_eq!(doc.elements.len(), 3);
        assert_eq!(doc.elements[0].text, "stray one");
        assert_eq!(doc.elements[1].text, "Body.");
        assert_eq!(doc.elements[2].text, "stray two");
        assert_eq!(
            doc.processing_warnings.len(),
            1,
            "repeated stray text should collapse to one warning: {:?}",
            doc.processing_warnings
        );
    }

    #[test]
    fn should_preserve_all_table_rows_when_a_close_tag_is_misnested() {
        // The stray `</text>` here has no matching `<text>` open; a
        // name-blind depth counter would still treat it as closing the
        // `<otsl>` early and lose the `y` row.
        let doc = parse_doctags("<doctag><otsl><ched>A<nl><fcel>x</text><nl><fcel>y<nl></otsl>\n</doctag>");
        let table = &doc.tables[0];
        assert_eq!(
            table.cells,
            vec![vec!["A".to_string()], vec!["x".to_string()], vec!["y".to_string()],]
        );
    }
}
