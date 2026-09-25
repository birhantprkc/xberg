//! Render an `InternalDocument` to Docling DocTags.
//!
//! DocTags is the tag vocabulary used by Docling and the SmolDocling /
//! Granite-Docling models. The document is wrapped in `<doctag>`, one element
//! per line, with tables encoded as OTSL.
//!
//! `<loc_*>` tokens are emitted for an element when it carries a bounding box
//! and its page recorded usable dimensions. In practice that means PDF today:
//! it is the only path that populates both. Pure-text formats (Markdown, HTML)
//! have no geometry at all and so carry no location tokens — a documented,
//! deliberate degradation rather than a silent one.
//!
//! One limitation remains, inherited from the document model rather than from
//! this renderer:
//!
//! - **OTSL merge tokens are never emitted.** `Table::cells` is a flat
//!   `Vec<Vec<String>>` with no span information, so a table can only be
//!   serialised as `<ched>`/`<fcel>`/`<ecel>` plus `<nl>` row separators.
//!   `<lcel>`/`<ucel>`/`<xcel>`/`<rhed>` require cell spans the model does not
//!   carry.
//!
//! Text is emitted verbatim. DocTags has no escaping mechanism — Docling's own
//! serialiser writes `&` and `<` literally, and its tokenizer only recognises
//! known tags — so escaping here would corrupt round-trips.

use crate::types::document_structure::{ContentLayer, RelationshipKind};
use crate::types::extraction::BoundingBox;
use crate::types::internal::{ElementKind, InternalDocument, RelationshipTarget};
use ahash::AHashMap;

use super::common::{get_admonition_kind, get_admonition_title, normalize_inline_text, parse_metadata_entries};

/// Language token emitted for a code block with no recorded language.
const UNKNOWN_LANGUAGE: &str = "<_unknown_>";

/// DocTags normalises bounding boxes onto a fixed square grid of this size.
const LOC_GRID: f64 = 500.0;

/// Rough average serialised length of one element in bytes, used only to
/// presize the output buffer and avoid reallocation churn on typical
/// documents. Not a correctness bound — the buffer grows past this freely.
const AVG_ELEMENT_BYTES: usize = 96;

/// Render an `InternalDocument` to Docling DocTags.
pub(crate) fn render_doctags(doc: &InternalDocument) -> String {
    let mut out = String::with_capacity(doc.elements.len() * AVG_ELEMENT_BYTES);
    let captions = collect_captions(doc);
    let dims = page_dimensions(doc);

    out.push_str("<doctag>");

    let mut state = ListState::default();

    for (index, elem) in doc.elements.iter().enumerate() {
        let index = index as u32;

        // Captions are emitted nested inside the element they describe.
        if captions.sources.contains_key(&index) {
            continue;
        }

        let loc = element_loc(elem, &dims);
        let loc = loc.as_deref();

        match elem.kind {
            ElementKind::ListItem { ordered } => {
                state.open(&mut out, ordered);
                // DocTags' `list_item` tag carries no separate marker slot -- a literal
                // source label (e.g. "B.", "(a)") that the auto `ordered` container alone
                // cannot express is prefixed onto the visible text instead, exactly as the
                // other text-based renderers (comrak/markdown, djot, plain) do.
                let text = match elem.list_item_source_label() {
                    Some(label) if !label.is_empty() => format!("{label} {}", elem.text),
                    _ => elem.text.clone(),
                };
                push_element(&mut out, "list_item", loc, &normalize_inline_text(&text), None);
                continue;
            }
            ElementKind::ListStart { ordered } => {
                state.open_explicit(&mut out, ordered);
                continue;
            }
            ElementKind::ListEnd => {
                state.close_explicit(&mut out);
                continue;
            }
            _ => state.close_implicit(&mut out),
        }

        match elem.kind {
            ElementKind::QuoteStart | ElementKind::QuoteEnd | ElementKind::GroupStart | ElementKind::GroupEnd => {}
            // The marker itself carries no content DocTags can address — the reference is
            // resolved through `Relationship`, and the definition is emitted on its own below.
            //
            // `CommentDefinition` is deliberately NOT dropped alongside these, which is where
            // this diverges from #1408. That fix reasoned that plain, djot and the comrak
            // bridge drop reviewer comments here too — true of this first match, but each of
            // them re-emits the body in a later pass (plain.rs:232 for the footnote layer,
            // djot.rs:259, comrak_bridge.rs:1024), and json.rs:385 / html_styled.rs:369 emit
            // it directly. This renderer has no second pass, so dropping it here would make
            // DocTags the only renderer that loses the comment text outright. It falls
            // through to the text arm below instead.
            ElementKind::FootnoteRef | ElementKind::CommentRef => {}
            ElementKind::PageBreak => {
                out.push_str("<page_break>\n");
            }
            ElementKind::Table { table_index } => {
                let rendered = match doc.tables.get(table_index as usize) {
                    Some(table) => {
                        let caption = captions.caption_payload(doc, index, &dims);
                        push_otsl(&mut out, &table.cells, loc, caption.as_deref())
                    }
                    None => false,
                };
                // `push_otsl` drops empty/degenerate tables (no cells, or no columns).
                // Unlike Image and Code, which always call `push_element` and so always
                // carry a nested `<caption>`, a dropped table takes its caption down with
                // it unless it is rescued here. This mirrors the parser's own philosophy
                // (`extraction/doctags.rs`): a caption whose target was dropped still
                // carries text, so it stays as an ordinary text element.
                if !rendered {
                    push_orphaned_caption(&mut out, doc, &captions, index, &dims);
                }
            }
            ElementKind::Image { image_index } => {
                let described = doc
                    .images
                    .get(image_index as usize)
                    .and_then(|img| img.description.as_deref())
                    .filter(|desc| !desc.is_empty());
                let caption = captions
                    .caption_payload(doc, index, &dims)
                    .or_else(|| described.map(|desc| normalize_inline_text(desc).into_owned()));
                push_element(&mut out, "picture", loc, "", caption.as_deref());
            }
            ElementKind::Code => {
                let body = code_body(super::common::get_language(elem), &elem.text);
                let caption = captions.caption_payload(doc, index, &dims);
                push_element(&mut out, "code", loc, &body, caption.as_deref());
            }
            ElementKind::Formula => {
                push_element(&mut out, "formula", loc, &normalize_inline_text(&elem.text), None);
            }
            ElementKind::Admonition => {
                // `InternalDocumentBuilder::push_admonition` stores exactly one string —
                // `elem.text` is set to `title.unwrap_or(kind)` at construction, and
                // `get_admonition_title`/`get_admonition_kind` read the same "title"/"kind"
                // attributes back out. There is no separate body to distinguish from the
                // label, and DocTags itself has no admonition/callout tag (the vendored
                // Docling corpus has none), so this renders as an ordinary text element,
                // exactly once. ~keep
                let label = get_admonition_title(elem).unwrap_or_else(|| get_admonition_kind(elem));
                push_text_element(&mut out, elem.layer, loc, &normalize_inline_text(label));
            }
            ElementKind::MetadataBlock => {
                let entries = parse_metadata_entries(&elem.text);
                if entries.is_empty() {
                    push_text_element(&mut out, elem.layer, loc, &normalize_inline_text(&elem.text));
                } else {
                    for (key, value) in entries {
                        push_text_element(&mut out, elem.layer, loc, &format!("{}: {}", key, value));
                    }
                }
            }
            ElementKind::RawBlock => {
                let body = code_body(None, &elem.text);
                push_element(&mut out, "code", loc, &body, None);
            }
            ElementKind::Title => {
                push_labelled(&mut out, elem.layer, loc, "title", &normalize_inline_text(&elem.text));
            }
            ElementKind::Heading { level } => {
                let tag = format!("section_header_level_{}", level.max(1));
                push_labelled(&mut out, elem.layer, loc, &tag, &normalize_inline_text(&elem.text));
            }
            ElementKind::FootnoteDefinition => {
                push_element(&mut out, "footnote", loc, &normalize_inline_text(&elem.text), None);
            }
            // DocTags has no comment tag, so the definition renders through its content
            // layer: `<footnote>` when the extractor marked it as such, `<text>` otherwise.
            // Dropping it instead would lose the text outright — unlike the plain and comrak
            // renderers, this one has no second pass that re-emits comment bodies.
            ElementKind::CommentDefinition
            | ElementKind::Paragraph
            | ElementKind::Citation
            | ElementKind::Slide { .. }
            | ElementKind::DefinitionTerm
            | ElementKind::DefinitionDescription
            | ElementKind::OcrText { .. } => {
                push_text_element(&mut out, elem.layer, loc, &normalize_inline_text(&elem.text));
            }
            // Handled above.
            ElementKind::ListItem { .. } | ElementKind::ListStart { .. } | ElementKind::ListEnd => {}
        }
    }

    state.close_implicit(&mut out);
    state.close_all(&mut out);

    out.push_str("</doctag>");
    out
}

/// Tracks open `<ordered_list>` / `<unordered_list>` wrappers.
///
/// Extractors emit list items either wrapped in explicit `ListStart`/`ListEnd`
/// markers or bare. Bare items open an implicit wrapper that closes at the next
/// non-list element.
#[derive(Default)]
struct ListState {
    explicit: Vec<bool>,
    implicit: Option<bool>,
}

impl ListState {
    fn open(&mut self, out: &mut String, ordered: bool) {
        if !self.explicit.is_empty() {
            return;
        }
        // A bare item whose ordering differs from the currently open implicit
        // wrapper (no explicit ListStart/ListEnd in between) must close that
        // wrapper and open a new one of the right kind, rather than being
        // silently absorbed into the wrong one.
        if self.implicit == Some(ordered) {
            return;
        }
        self.close_implicit(out);
        push_list_open(out, ordered);
        self.implicit = Some(ordered);
    }

    fn open_explicit(&mut self, out: &mut String, ordered: bool) {
        self.close_implicit(out);
        push_list_open(out, ordered);
        self.explicit.push(ordered);
    }

    fn close_explicit(&mut self, out: &mut String) {
        if let Some(ordered) = self.explicit.pop() {
            push_list_close(out, ordered);
        }
    }

    fn close_implicit(&mut self, out: &mut String) {
        if let Some(ordered) = self.implicit.take() {
            push_list_close(out, ordered);
        }
    }

    fn close_all(&mut self, out: &mut String) {
        while let Some(ordered) = self.explicit.pop() {
            push_list_close(out, ordered);
        }
    }
}

fn push_list_open(out: &mut String, ordered: bool) {
    if ordered {
        out.push_str("<ordered_list>");
    } else {
        out.push_str("<unordered_list>");
    }
}

fn push_list_close(out: &mut String, ordered: bool) {
    if ordered {
        out.push_str("</ordered_list>\n");
    } else {
        out.push_str("</unordered_list>\n");
    }
}

/// Per-page dimensions in points, keyed by 1-indexed page number.
///
/// Only pages that recorded usable dimensions are included; the emptiness of
/// this map is what suppresses location tokens for formats that have none.
fn page_dimensions(doc: &InternalDocument) -> AHashMap<u32, (f64, f64)> {
    let mut dims = AHashMap::new();
    let Some(pages) = doc.metadata.pages.as_ref().and_then(|pages| pages.pages.as_ref()) else {
        return dims;
    };
    for page in pages {
        let Some(dimensions) = page.dimensions else {
            continue;
        };
        let width = dimensions.width;
        let height = dimensions.height;
        if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
            dims.insert(page.number, (width, height));
        }
    }
    dims
}

/// Render an element's `<loc_*>` tokens, or `None` when geometry is unusable.
///
/// Input boxes follow the `BoundingBox` contract — PDF user space, origin at
/// the bottom-left, `y0` the bottom edge and `y1` the top. DocTags counts from
/// the top-left, so the vertical axis is flipped here. Tokens are emitted as
/// left, top, right, bottom.
///
/// Formats whose bounding boxes do not follow that contract must not reach this
/// function. PPTX is the live example: `extraction::pptx` fills `y0` with the
/// *top* edge, so its geometry would be flipped. It is excluded structurally
/// rather than by a format check — PPTX never records page dimensions, so
/// `page_dimensions` yields nothing for it and no tokens are produced.
fn loc_tokens(bbox: &BoundingBox, (width, height): (f64, f64)) -> Option<String> {
    let coords = [bbox.x0, bbox.y0, bbox.x1, bbox.y1];
    if coords.iter().any(|value| !value.is_finite()) {
        return None;
    }

    // Tolerate boxes recorded with reversed corners.
    let left = bbox.x0.min(bbox.x1);
    let right = bbox.x0.max(bbox.x1);
    let bottom = bbox.y0.min(bbox.y1);
    let top = bbox.y0.max(bbox.y1);

    let to_grid = |value: f64, extent: f64| -> u32 {
        let scaled = (value / extent * LOC_GRID).round();
        scaled.clamp(0.0, LOC_GRID) as u32
    };

    Some(format!(
        "<loc_{}><loc_{}><loc_{}><loc_{}>",
        to_grid(left, width),
        to_grid(height - top, height),
        to_grid(right, width),
        to_grid(height - bottom, height),
    ))
}

/// Resolve an element's location tokens against its page's dimensions.
fn element_loc(elem: &crate::types::internal::InternalElement, dims: &AHashMap<u32, (f64, f64)>) -> Option<String> {
    let bbox = elem.bbox.as_ref()?;
    let page = elem.page?;
    loc_tokens(bbox, *dims.get(&page)?)
}

/// Build a `<code>` body: a language token followed by the flattened source.
fn code_body(language: Option<&str>, text: &str) -> String {
    let mut body = match language {
        Some(language) => format!("<_{}_>", language),
        None => UNKNOWN_LANGUAGE.to_string(),
    };
    body.push_str(&normalize_inline_text(text));
    body
}

/// Emit an element: `<tag>` then location tokens, body, and a nested
/// `<caption>` when present.
fn push_element(out: &mut String, tag: &str, loc: Option<&str>, body: &str, caption: Option<&str>) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    if let Some(loc) = loc {
        out.push_str(loc);
    }
    out.push_str(body);
    if let Some(caption) = caption {
        out.push_str("<caption>");
        out.push_str(caption);
        out.push_str("</caption>");
    }
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

/// Emit a prose element, letting the content layer choose the tag.
fn push_text_element(out: &mut String, layer: ContentLayer, loc: Option<&str>, text: &str) {
    if text.is_empty() {
        return;
    }
    push_element(out, layer_tag(layer, "text"), loc, text, None);
}

/// Emit an element whose tag is overridden by a non-body content layer.
fn push_labelled(out: &mut String, layer: ContentLayer, loc: Option<&str>, tag: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    push_element(out, layer_tag(layer, tag), loc, text, None);
}

/// Map a content layer onto its DocTags tag, falling back to `body_tag`.
fn layer_tag(layer: ContentLayer, body_tag: &str) -> &str {
    match layer {
        ContentLayer::Body => body_tag,
        ContentLayer::Header => "page_header",
        ContentLayer::Footer => "page_footer",
        ContentLayer::Footnote => "footnote",
    }
}

/// Serialise a flat cell grid as OTSL.
///
/// The first row is treated as the header (matching `render_table_markdown`).
/// Ragged rows are padded with `<ecel>` so every row has the same cell count,
/// which OTSL requires.
///
/// Returns `false` without writing anything when the table is empty or has no
/// columns — callers must handle that case (e.g. a caption that would
/// otherwise have nested inside the dropped `<otsl>`).
fn push_otsl(out: &mut String, cells: &[Vec<String>], loc: Option<&str>, caption: Option<&str>) -> bool {
    if cells.is_empty() {
        return false;
    }
    let columns = cells.iter().map(|row| row.len()).max().unwrap_or(0);
    if columns == 0 {
        return false;
    }

    out.push_str("<otsl>");
    if let Some(loc) = loc {
        out.push_str(loc);
    }
    for (row_index, row) in cells.iter().enumerate() {
        for column in 0..columns {
            let content = row.get(column).map(|s| s.trim()).unwrap_or("");
            if content.is_empty() {
                out.push_str("<ecel>");
            } else {
                out.push_str(if row_index == 0 { "<ched>" } else { "<fcel>" });
                out.push_str(&normalize_inline_text(content));
            }
        }
        out.push_str("<nl>");
    }
    if let Some(caption) = caption {
        out.push_str("<caption>");
        out.push_str(caption);
        out.push_str("</caption>");
    }
    out.push_str("</otsl>\n");
    true
}

/// Caption relationships, indexed both ways.
struct Captions {
    /// Caption element index → the element it describes.
    sources: AHashMap<u32, u32>,
    /// Described element index → its caption element index.
    targets: AHashMap<u32, u32>,
}

impl Captions {
    /// The inner content of a `<caption>`: its own location tokens, then text.
    fn caption_payload(&self, doc: &InternalDocument, target: u32, dims: &AHashMap<u32, (f64, f64)>) -> Option<String> {
        let source = *self.targets.get(&target)?;
        let elem = doc.elements.get(source as usize)?;
        let text = normalize_inline_text(&elem.text);
        if text.is_empty() {
            return None;
        }
        Some(match element_loc(elem, dims) {
            Some(loc) => format!("{}{}", loc, text),
            None => text.into_owned(),
        })
    }
}

/// Render a caption as an ordinary text element when its target did not
/// render (e.g. an empty table dropped by `push_otsl`).
///
/// Mirrors `extraction::doctags`'s parse-side behaviour: a caption whose
/// target was dropped still carries text, so it stays as a plain element
/// rather than being discarded along with the target it described.
fn push_orphaned_caption(
    out: &mut String,
    doc: &InternalDocument,
    captions: &Captions,
    target: u32,
    dims: &AHashMap<u32, (f64, f64)>,
) {
    let Some(&source) = captions.targets.get(&target) else {
        return;
    };
    let Some(elem) = doc.elements.get(source as usize) else {
        return;
    };
    let loc = element_loc(elem, dims);
    push_text_element(out, elem.layer, loc.as_deref(), &normalize_inline_text(&elem.text));
}

fn collect_captions(doc: &InternalDocument) -> Captions {
    let mut sources = AHashMap::new();
    let mut targets = AHashMap::new();

    for rel in &doc.relationships {
        if rel.kind != RelationshipKind::Caption {
            continue;
        }
        let RelationshipTarget::Index(target) = rel.target else {
            continue;
        };
        // Only tags that nest a `<caption>` may claim one. Captions are also
        // attached to plain paragraphs standing in for a figure (LaTeX
        // `figure` with placeholder injection); those must keep rendering as
        // their own element rather than being silently swallowed.
        let nests_caption = doc.elements.get(target as usize).is_some_and(|elem| {
            matches!(
                elem.kind,
                ElementKind::Table { .. } | ElementKind::Image { .. } | ElementKind::Code
            )
        });
        if !nests_caption {
            continue;
        }
        // First caption wins, so a described element never gains two captions.
        if targets.contains_key(&target) {
            continue;
        }
        sources.insert(rel.source, target);
        targets.insert(target, rel.source);
    }

    Captions { sources, targets }
}

/// Structural validation of a DocTags stream.
///
/// Kept as independent checks rather than one pass so each can be applied where
/// it is actually valid. The vendored Docling corpus predates OTSL in places and
/// still carries legacy `<table>`/`<tr>`/`<td>` markup, so vocabulary and OTSL
/// checks cannot run against it, while wrapper and location invariants can.
#[cfg(test)]
mod validate {
    /// Cell tokens that occupy one OTSL grid position.
    const OTSL_CELLS: &[&str] = &["fcel", "ched", "ecel", "lcel", "ucel", "xcel", "rhed"];

    /// Tokens that stand alone rather than wrapping content.
    const STANDALONE: &[&str] = &["nl", "page_break"];

    /// Tags that must open and close.
    ///
    /// `checkbox_*` wrap their label text in real Docling output, e.g.
    /// `<checkbox_unselected><loc_…>بلی</checkbox_unselected>`, so they pair.
    const PAIRED: &[&str] = &[
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

    #[derive(Debug, PartialEq)]
    pub(super) enum Token<'a> {
        Open(&'a str),
        Close(&'a str),
    }

    /// Extract tag tokens in order. Text content between tags is ignored.
    ///
    /// Only *recognised* tag names count as tags. DocTags has no escaping, and
    /// real Docling output carries literal `<` in prose (the vendored
    /// `2203.01017v2` corpus discusses `< td >` in a caption). Scanning for the
    /// next `>` would swallow the real closing tag that follows such text, so
    /// anything that is not a known token is treated as content.
    pub(super) fn tags(doc: &str) -> Vec<Token<'_>> {
        let mut out = Vec::new();
        let bytes = doc.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            let Some(end) = doc[i..].find('>') else { break };
            let name = &doc[i + 1..i + end];
            let token = match name.strip_prefix('/') {
                Some(closing) => Token::Close(closing),
                None => Token::Open(name),
            };
            let recognised = match token {
                Token::Open(name) | Token::Close(name) => is_standalone(name) || is_paired(name),
            };
            if !recognised {
                i += 1;
                continue;
            }
            out.push(token);
            i += end + 1;
        }
        out
    }

    fn is_standalone(name: &str) -> bool {
        STANDALONE.contains(&name)
            || OTSL_CELLS.contains(&name)
            || name.starts_with("loc_")
            // Code language tokens, e.g. `<_rust_>` and `<_unknown_>`.
            || (name.starts_with('_') && name.ends_with('_') && name.len() > 1)
    }

    fn is_paired(name: &str) -> bool {
        PAIRED.contains(&name) || name.starts_with("section_header_level_")
    }

    /// Every angle-bracketed run is a tag this format defines.
    ///
    /// Unlike [`tags`], this scans raw and refuses to skip what it does not
    /// recognise, which is the point: it catches a tag we emitted by mistake.
    /// Only meaningful for output built from content with no stray `<`, since
    /// the format cannot distinguish the two.
    pub(super) fn vocabulary(doc: &str) -> Result<(), String> {
        let mut rest = doc;
        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            let Some(end) = rest.find('>') else { break };
            let name = &rest[..end];
            let name = name.strip_prefix('/').unwrap_or(name);
            if !is_standalone(name) && !is_paired(name) {
                return Err(format!("unknown tag <{}>", name));
            }
            rest = &rest[end + 1..];
        }
        Ok(())
    }

    /// Paired tags nest correctly and the document is wrapped in `<doctag>`.
    pub(super) fn wrapper_and_nesting(doc: &str) -> Result<(), String> {
        if !doc.starts_with("<doctag>") {
            return Err("missing opening <doctag>".to_string());
        }
        if !doc.ends_with("</doctag>") {
            return Err("missing closing </doctag>".to_string());
        }

        let mut stack: Vec<&str> = Vec::new();
        for token in tags(doc) {
            match token {
                Token::Open(name) if is_paired(name) => stack.push(name),
                Token::Close(name) => match stack.pop() {
                    Some(open) if open == name => {}
                    Some(open) => return Err(format!("</{}> closes <{}>", name, open)),
                    None => return Err(format!("</{}> with nothing open", name)),
                },
                _ => {}
            }
        }
        if let Some(unclosed) = stack.pop() {
            return Err(format!("<{}> never closed", unclosed));
        }
        Ok(())
    }

    /// Location tokens come in fours, sit on the 0–500 grid, and are ordered
    /// left ≤ right and top ≤ bottom.
    pub(super) fn location_tokens(doc: &str) -> Result<(), String> {
        let mut group: Vec<u32> = Vec::with_capacity(4);
        for token in tags(doc) {
            let Token::Open(name) = token else { continue };
            let Some(value) = name.strip_prefix("loc_") else {
                if !group.is_empty() && group.len() < 4 {
                    return Err(format!("location group of {}, expected 4", group.len()));
                }
                group.clear();
                continue;
            };
            let value: u32 = value.parse().map_err(|_| format!("malformed <{}>", name))?;
            if value > 500 {
                return Err(format!("<{}> exceeds the 0-500 grid", name));
            }
            group.push(value);
            if group.len() == 4 {
                if group[0] > group[2] {
                    return Err(format!("left {} right {}", group[0], group[2]));
                }
                if group[1] > group[3] {
                    return Err(format!("top {} bottom {}", group[1], group[3]));
                }
                group.clear();
            }
        }
        if !group.is_empty() {
            return Err(format!("trailing location group of {}", group.len()));
        }
        Ok(())
    }

    /// Every OTSL row holds the same number of cells.
    pub(super) fn otsl_rows(doc: &str) -> Result<(), String> {
        let mut in_otsl = false;
        let mut row = 0usize;
        let mut expected: Option<usize> = None;
        for token in tags(doc) {
            match token {
                Token::Open("otsl") => {
                    in_otsl = true;
                    row = 0;
                    expected = None;
                }
                Token::Close("otsl") => {
                    if row != 0 {
                        return Err("OTSL row not terminated by <nl>".to_string());
                    }
                    in_otsl = false;
                }
                Token::Open("nl") if in_otsl => {
                    match expected {
                        Some(width) if width != row => {
                            return Err(format!("OTSL row of {} cells, expected {}", row, width));
                        }
                        None => expected = Some(row),
                        _ => {}
                    }
                    row = 0;
                }
                Token::Open(name) if in_otsl && OTSL_CELLS.contains(&name) => row += 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// All checks, for output this renderer produced.
    pub(super) fn strict(doc: &str) -> Result<(), String> {
        vocabulary(doc)?;
        wrapper_and_nesting(doc)?;
        location_tokens(doc)?;
        otsl_rows(doc)
    }
}

#[cfg(test)]
mod tests;
