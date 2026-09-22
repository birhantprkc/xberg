//! DOCX style resolution from `word/styles.xml`.
//!
//! Parses the styles XML and resolves style inheritance chains to produce
//! fully-flattened run and paragraph properties for any given style ID.

use ahash::AHashMap;

use crate::error::{Result, XbergError};
use crate::extraction::ooxml_constants::WORDPROCESSINGML_NAMESPACE;
// --- Types ---

/// The type of a style definition in DOCX.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, PartialEq)]
pub enum StyleType {
    /// Style applied to an entire paragraph (`<w:style w:type="paragraph">`).
    Paragraph,
    /// Style applied to a character run (`<w:style w:type="character">`).
    Character,
    /// Style applied to a table (`<w:style w:type="table">`).
    Table,
    /// Style applied to a list numbering definition (`<w:style w:type="numbering">`).
    Numbering,
}
/// Run-level formatting properties (bold, italic, font, size, color, etc.).
///
/// All fields are `Option` so that inheritance resolution can distinguish
/// "not set" (`None`) from "explicitly set" (`Some`).
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunProperties {
    /// Bold formatting.
    pub bold: Option<bool>,
    /// Italic formatting.
    pub italic: Option<bool>,
    /// Underline formatting.
    pub underline: Option<bool>,
    /// Strikethrough formatting.
    pub strikethrough: Option<bool>,
    /// Hex RGB color, e.g. `"2F5496"`.
    pub color: Option<String>,
    /// Font size in half-points (`w:sz` val). Divide by 2 to get points.
    pub font_size_half_points: Option<i32>,
    /// ASCII font family (`w:rFonts w:ascii`).
    pub font_ascii: Option<String>,
    /// ASCII theme font (`w:rFonts w:asciiTheme`).
    pub font_ascii_theme: Option<String>,
    /// Vertical alignment: "superscript", "subscript", or "baseline".
    pub vert_align: Option<String>,
    /// High ANSI font family (w:rFonts w:hAnsi).
    pub font_h_ansi: Option<String>,
    /// Complex script font family (w:rFonts w:cs).
    pub font_cs: Option<String>,
    /// East Asian font family (w:rFonts w:eastAsia).
    pub font_east_asia: Option<String>,
    /// Highlight color name (e.g., "yellow", "green", "cyan").
    pub highlight: Option<String>,
    /// All caps text transformation.
    pub caps: Option<bool>,
    /// Small caps text transformation.
    pub small_caps: Option<bool>,
    /// Text shadow effect.
    pub shadow: Option<bool>,
    /// Text outline effect.
    pub outline: Option<bool>,
    /// Text emboss effect.
    pub emboss: Option<bool>,
    /// Text imprint (engrave) effect.
    pub imprint: Option<bool>,
    /// Character spacing in twips (from w:spacing w:val).
    pub char_spacing: Option<i32>,
    /// Vertical position offset in half-points (from w:position w:val).
    pub position: Option<i32>,
    /// Kerning threshold in half-points (from w:kern w:val).
    pub kern: Option<i32>,
    /// Theme color reference (e.g., "accent1", "dk1").
    pub theme_color: Option<String>,
    /// Theme color tint modification (hex value).
    pub theme_tint: Option<String>,
    /// Theme color shade modification (hex value).
    pub theme_shade: Option<String>,
}
#[cfg_attr(alef, alef(skip))]
/// Paragraph-level formatting properties (alignment, spacing, indentation, etc.).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParagraphProperties {
    /// `"left"`, `"center"`, `"right"`, `"both"` (justified).
    pub alignment: Option<String>,
    /// Spacing before paragraph in twips.
    pub spacing_before: Option<i32>,
    /// Spacing after paragraph in twips.
    pub spacing_after: Option<i32>,
    /// Line spacing in twips or 240ths of a line.
    pub spacing_line: Option<i32>,
    /// Line spacing rule: "auto", "exact", or "atLeast".
    pub spacing_line_rule: Option<String>,
    /// Left indentation in twips.
    pub indent_left: Option<i32>,
    /// Right indentation in twips.
    pub indent_right: Option<i32>,
    /// First-line indentation in twips.
    pub indent_first_line: Option<i32>,
    /// Hanging indentation in twips.
    pub indent_hanging: Option<i32>,
    /// Outline level 0-8 for heading levels.
    pub outline_level: Option<u8>,
    /// Numbering definition id (`w:numPr/w:numId`) the style applies.
    ///
    /// Word lets a list style carry the numbering reference instead of putting it
    /// on each paragraph, which is what the built-in `List Bullet` and
    /// `List Number` styles do.
    pub numbering_id: Option<i64>,
    /// Indentation level within that numbering definition (`w:numPr/w:ilvl`).
    ///
    /// Absent on most list styles. Word reads an absent level as 0.
    pub numbering_level: Option<i64>,
    /// Keep with next paragraph on same page.
    pub keep_next: Option<bool>,
    /// Keep all lines of paragraph on same page.
    pub keep_lines: Option<bool>,
    /// Force page break before paragraph.
    pub page_break_before: Option<bool>,
    /// Prevent widow/orphan lines.
    pub widow_control: Option<bool>,
    /// Suppress automatic hyphenation.
    pub suppress_auto_hyphens: Option<bool>,
    /// Right-to-left paragraph direction.
    pub bidi: Option<bool>,
    /// Background color hex value (from w:shd w:fill).
    pub shading_fill: Option<String>,
    /// Shading pattern value (from w:shd w:val).
    pub shading_val: Option<String>,
    /// Top border style (from w:pBdr/w:top w:val).
    pub border_top: Option<String>,
    /// Bottom border style (from w:pBdr/w:bottom w:val).
    pub border_bottom: Option<String>,
    /// Left border style (from w:pBdr/w:left w:val).
    pub border_left: Option<String>,
    /// Right border style (from w:pBdr/w:right w:val).
    pub border_right: Option<String>,
}

/// A single style definition parsed from `<w:style>` in `word/styles.xml`.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone)]
pub struct StyleDefinition {
    /// The style ID (`w:styleId` attribute).
    pub id: String,
    /// Human-readable name (`<w:name w:val="..."/>`).
    pub name: Option<String>,
    /// Style type: paragraph, character, table, or numbering.
    pub style_type: StyleType,
    /// ID of the parent style (`<w:basedOn w:val="..."/>`).
    pub based_on: Option<String>,
    /// ID of the style to apply to the next paragraph (`<w:next w:val="..."/>`).
    pub next_style: Option<String>,
    /// Whether this is the default style for its type.
    pub is_default: bool,
    /// Paragraph properties defined directly on this style.
    pub paragraph_properties: ParagraphProperties,
    /// Run properties defined directly on this style.
    pub run_properties: RunProperties,
}

/// Fully resolved (flattened) style after walking the inheritance chain.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedStyle {
    /// Fully-inherited paragraph formatting properties.
    pub paragraph_properties: ParagraphProperties,
    /// Fully-inherited run formatting properties.
    pub run_properties: RunProperties,
}

/// Catalog of all styles parsed from `word/styles.xml`, plus document defaults.
#[cfg_attr(alef, alef(skip))]
#[derive(Debug, Clone, Default)]
pub struct StyleCatalog {
    /// All named style definitions keyed by their style ID.
    pub styles: AHashMap<String, StyleDefinition>,
    /// Default paragraph formatting properties for the document.
    pub default_paragraph_properties: ParagraphProperties,
    /// Default run formatting properties for the document.
    pub default_run_properties: RunProperties,
}

/// Parse `word/styles.xml` content into a `StyleCatalog`.
///
/// Uses `roxmltree` for tree-based XML parsing, consistent with the
/// office metadata parsing approach used elsewhere in the codebase.
pub(crate) fn parse_styles_xml(xml: &str) -> Result<StyleCatalog> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| XbergError::parsing(format!("Failed to parse styles.xml: {}", e)))?;

    let root = doc.root_element();

    let mut catalog = StyleCatalog::default();

    for child in root.children() {
        if !child.is_element() {
            continue;
        }

        match child.tag_name().name() {
            "docDefaults" => parse_doc_defaults(&child, &mut catalog),
            "style" => {
                if let Some(style_def) = parse_style_element(&child) {
                    catalog.styles.insert(style_def.id.clone(), style_def);
                }
            }
            _ => {}
        }
    }

    Ok(catalog)
}

/// Parse `<w:docDefaults>` to extract default run and paragraph properties.
fn parse_doc_defaults(node: &roxmltree::Node, catalog: &mut StyleCatalog) {
    for child in node.children() {
        if !child.is_element() {
            continue;
        }

        match child.tag_name().name() {
            "rPrDefault" => {
                if let Some(rpr) = find_child_element(&child, "rPr") {
                    catalog.default_run_properties = parse_run_properties(&rpr);
                }
            }
            "pPrDefault" => {
                if let Some(ppr) = find_child_element(&child, "pPr") {
                    catalog.default_paragraph_properties = parse_paragraph_properties(&ppr);
                }
            }
            _ => {}
        }
    }
}

/// Parse a single `<w:style>` element into a `StyleDefinition`.
///
/// Returns `None` if the element lacks a `w:styleId` or `w:type` attribute.
fn parse_style_element(node: &roxmltree::Node) -> Option<StyleDefinition> {
    let style_id = node.attribute((WORDPROCESSINGML_NAMESPACE, "styleId"))?;

    let type_str = node.attribute((WORDPROCESSINGML_NAMESPACE, "type"))?;
    let style_type = match type_str {
        "paragraph" => StyleType::Paragraph,
        "character" => StyleType::Character,
        "table" => StyleType::Table,
        "numbering" => StyleType::Numbering,
        _ => return None,
    };

    let is_default = node
        .attribute((WORDPROCESSINGML_NAMESPACE, "default"))
        .is_some_and(|v| v == "1" || v == "true");

    let mut name = None;
    let mut based_on = None;
    let mut next_style = None;
    let mut paragraph_properties = ParagraphProperties::default();
    let mut run_properties = RunProperties::default();

    for child in node.children() {
        if !child.is_element() {
            continue;
        }

        match child.tag_name().name() {
            "name" => {
                name = get_w_val(&child).map(String::from);
            }
            "basedOn" => {
                based_on = get_w_val(&child).map(String::from);
            }
            "next" => {
                next_style = get_w_val(&child).map(String::from);
            }
            "pPr" => {
                paragraph_properties = parse_paragraph_properties(&child);
            }
            "rPr" => {
                run_properties = parse_run_properties(&child);
            }
            _ => {}
        }
    }

    Some(StyleDefinition {
        id: style_id.to_string(),
        name,
        style_type,
        based_on,
        next_style,
        is_default,
        paragraph_properties,
        run_properties,
    })
}

/// Parse `<w:rPr>` run properties from a node's children.
fn parse_run_properties(node: &roxmltree::Node) -> RunProperties {
    let mut props = RunProperties::default();

    for child in node.children() {
        if !child.is_element() {
            continue;
        }

        match child.tag_name().name() {
            "b" => props.bold = Some(parse_toggle_property(&child)),
            "i" => props.italic = Some(parse_toggle_property(&child)),
            "u" => {
                let val = get_w_val(&child);
                props.underline = Some(!matches!(val, Some("none")));
            }
            "strike" | "dstrike" => props.strikethrough = Some(parse_toggle_property(&child)),
            "color" => {
                props.color = get_w_val(&child).map(String::from);
                props.theme_color = get_w_attr_string(&child, "themeColor");
                props.theme_tint = get_w_attr_string(&child, "themeTint");
                props.theme_shade = get_w_attr_string(&child, "themeShade");
            }
            "sz" => {
                props.font_size_half_points = get_w_val(&child).and_then(|v| v.parse::<i32>().ok());
            }
            "rFonts" => {
                props.font_ascii = child.attribute((WORDPROCESSINGML_NAMESPACE, "ascii")).map(String::from);
                props.font_ascii_theme = child
                    .attribute((WORDPROCESSINGML_NAMESPACE, "asciiTheme"))
                    .map(String::from);
                props.font_h_ansi = child.attribute((WORDPROCESSINGML_NAMESPACE, "hAnsi")).map(String::from);
                props.font_cs = child.attribute((WORDPROCESSINGML_NAMESPACE, "cs")).map(String::from);
                props.font_east_asia = child
                    .attribute((WORDPROCESSINGML_NAMESPACE, "eastAsia"))
                    .map(String::from);
            }
            "vertAlign" => {
                props.vert_align = get_w_val(&child).map(String::from);
            }
            "highlight" => {
                props.highlight = get_w_val(&child).map(String::from);
            }
            "caps" => {
                props.caps = Some(parse_toggle_property(&child));
            }
            "smallCaps" => {
                props.small_caps = Some(parse_toggle_property(&child));
            }
            "shadow" => {
                props.shadow = Some(parse_toggle_property(&child));
            }
            "outline" => {
                props.outline = Some(parse_toggle_property(&child));
            }
            "emboss" => {
                props.emboss = Some(parse_toggle_property(&child));
            }
            "imprint" => {
                props.imprint = Some(parse_toggle_property(&child));
            }
            "spacing" => {
                props.char_spacing = get_w_val(&child).and_then(|v| v.parse::<i32>().ok());
            }
            "position" => {
                props.position = get_w_val(&child).and_then(|v| v.parse::<i32>().ok());
            }
            "kern" => {
                props.kern = get_w_val(&child).and_then(|v| v.parse::<i32>().ok());
            }
            _ => {}
        }
    }

    props
}

/// Parse `<w:pPr>` paragraph properties from a node's children.
fn parse_paragraph_properties(node: &roxmltree::Node) -> ParagraphProperties {
    let mut props = ParagraphProperties::default();

    for child in node.children() {
        if !child.is_element() {
            continue;
        }

        match child.tag_name().name() {
            "jc" => {
                props.alignment = get_w_val(&child).map(String::from);
            }
            "spacing" => {
                props.spacing_before = get_w_attr_i32(&child, "before");
                props.spacing_after = get_w_attr_i32(&child, "after");
                props.spacing_line = get_w_attr_i32(&child, "line");
                props.spacing_line_rule = get_w_attr_string(&child, "lineRule");
            }
            "ind" => {
                props.indent_left = get_w_attr_i32(&child, "left");
                props.indent_right = get_w_attr_i32(&child, "right");
                props.indent_first_line = get_w_attr_i32(&child, "firstLine");
                props.indent_hanging = get_w_attr_i32(&child, "hanging");
            }
            "outlineLvl" => {
                props.outline_level = get_w_val(&child).and_then(|v| v.parse::<u8>().ok());
            }
            "numPr" => {
                (props.numbering_id, props.numbering_level) = parse_numbering_properties(&child);
            }
            "keepNext" => {
                props.keep_next = Some(parse_toggle_property(&child));
            }
            "keepLines" => {
                props.keep_lines = Some(parse_toggle_property(&child));
            }
            "pageBreakBefore" => {
                props.page_break_before = Some(parse_toggle_property(&child));
            }
            "widowControl" => {
                props.widow_control = Some(parse_toggle_property(&child));
            }
            "suppressAutoHyphens" => {
                props.suppress_auto_hyphens = Some(parse_toggle_property(&child));
            }
            "bidi" => {
                props.bidi = Some(parse_toggle_property(&child));
            }
            "shd" => {
                props.shading_fill = get_w_attr_string(&child, "fill");
                props.shading_val = get_w_attr_string(&child, "val");
            }
            "pBdr" => {
                for border_child in child.children() {
                    if !border_child.is_element() {
                        continue;
                    }
                    match border_child.tag_name().name() {
                        "top" => props.border_top = get_w_val(&border_child).map(String::from),
                        "bottom" => props.border_bottom = get_w_val(&border_child).map(String::from),
                        "left" | "start" => props.border_left = get_w_val(&border_child).map(String::from),
                        "right" | "end" => props.border_right = get_w_val(&border_child).map(String::from),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    props
}

/// Parse `<w:numPr>` (numbering id and indent level) from a paragraph property's children.
fn parse_numbering_properties(node: &roxmltree::Node) -> (Option<i64>, Option<i64>) {
    let mut numbering_id = None;
    let mut numbering_level = None;

    for child in node.children() {
        if !child.is_element() {
            continue;
        }
        match child.tag_name().name() {
            "numId" => numbering_id = get_w_val(&child).and_then(|v| v.parse::<i64>().ok()),
            "ilvl" => numbering_level = get_w_val(&child).and_then(|v| v.parse::<i64>().ok()),
            _ => {}
        }
    }

    (numbering_id, numbering_level)
}

/// Get the `w:val` attribute from a node.
///
/// Tries namespaced attribute first, then falls back to unqualified `val`.
fn get_w_val<'a>(node: &'a roxmltree::Node) -> Option<&'a str> {
    node.attribute((WORDPROCESSINGML_NAMESPACE, "val"))
        .or_else(|| node.attribute("val"))
}

/// Get a namespaced integer attribute from a node (e.g., `w:before`, `w:left`).
fn get_w_attr_i32(node: &roxmltree::Node, local_name: &str) -> Option<i32> {
    node.attribute((WORDPROCESSINGML_NAMESPACE, local_name))
        .or_else(|| node.attribute(local_name))
        .and_then(|v| v.parse::<i32>().ok())
}

/// Get a namespaced string attribute from a node.
fn get_w_attr_string(node: &roxmltree::Node, local_name: &str) -> Option<String> {
    node.attribute((WORDPROCESSINGML_NAMESPACE, local_name))
        .or_else(|| node.attribute(local_name))
        .map(String::from)
}

/// Find the first child element with the given local name.
fn find_child_element<'a>(node: &'a roxmltree::Node, local_name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == local_name)
}

/// Parse a toggle (boolean) property element.
///
/// - `<w:b/>` (no val attribute) -> `true`
/// - `<w:b w:val="1"/>` or `<w:b w:val="true"/>` -> `true`
/// - `<w:b w:val="0"/>` or `<w:b w:val="false"/>` -> `false`
fn parse_toggle_property(node: &roxmltree::Node) -> bool {
    match get_w_val(node) {
        None => true,
        Some(val) => !matches!(val, "0" | "false"),
    }
}
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
