use super::*;

/// Namespace prefix for building test XML.
const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn make_styles_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="{W_NS}">
{body}
</w:styles>"#
    )
}

#[test]
fn test_parse_empty_styles() {
    let xml = make_styles_xml("");
    let catalog = parse_styles_xml(&xml).expect("should parse empty styles");
    assert!(catalog.styles.is_empty());
    assert_eq!(catalog.default_run_properties, RunProperties::default());
    assert_eq!(catalog.default_paragraph_properties, ParagraphProperties::default());
}

#[test]
fn test_parse_doc_defaults() {
    let xml = make_styles_xml(
        r#"
        <w:docDefaults>
            <w:rPrDefault>
                <w:rPr>
                    <w:sz w:val="24"/>
                    <w:rFonts w:ascii="Calibri" w:asciiTheme="minorHAnsi"/>
                </w:rPr>
            </w:rPrDefault>
            <w:pPrDefault>
                <w:pPr>
                    <w:spacing w:after="160" w:line="259"/>
                </w:pPr>
            </w:pPrDefault>
        </w:docDefaults>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse doc defaults");

    assert_eq!(catalog.default_run_properties.font_size_half_points, Some(24));
    assert_eq!(catalog.default_run_properties.font_ascii.as_deref(), Some("Calibri"));
    assert_eq!(
        catalog.default_run_properties.font_ascii_theme.as_deref(),
        Some("minorHAnsi")
    );
    assert_eq!(catalog.default_paragraph_properties.spacing_after, Some(160));
    assert_eq!(catalog.default_paragraph_properties.spacing_line, Some(259));
}

#[test]
fn test_parse_style_definitions() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
            <w:name w:val="Normal"/>
            <w:pPr>
                <w:spacing w:after="200"/>
                <w:jc w:val="left"/>
            </w:pPr>
            <w:rPr>
                <w:sz w:val="22"/>
            </w:rPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Heading1">
            <w:name w:val="heading 1"/>
            <w:basedOn w:val="Normal"/>
            <w:next w:val="Normal"/>
            <w:pPr>
                <w:keepNext/>
                <w:keepLines/>
                <w:spacing w:before="240"/>
                <w:outlineLvl w:val="0"/>
            </w:pPr>
            <w:rPr>
                <w:b/>
                <w:color w:val="2F5496"/>
                <w:sz w:val="32"/>
                <w:rFonts w:asciiTheme="majorHAnsi"/>
            </w:rPr>
        </w:style>
        <w:style w:type="character" w:styleId="Strong">
            <w:name w:val="Strong"/>
            <w:rPr>
                <w:b/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse style definitions");

    assert_eq!(catalog.styles.len(), 3);

    let normal = catalog.styles.get("Normal").expect("Normal style must exist");
    assert_eq!(normal.style_type, StyleType::Paragraph);
    assert!(normal.is_default);
    assert_eq!(normal.name.as_deref(), Some("Normal"));
    assert_eq!(normal.paragraph_properties.spacing_after, Some(200));
    assert_eq!(normal.paragraph_properties.alignment.as_deref(), Some("left"));
    assert_eq!(normal.run_properties.font_size_half_points, Some(22));

    let heading1 = catalog.styles.get("Heading1").expect("Heading1 style must exist");
    assert_eq!(heading1.style_type, StyleType::Paragraph);
    assert!(!heading1.is_default);
    assert_eq!(heading1.name.as_deref(), Some("heading 1"));
    assert_eq!(heading1.based_on.as_deref(), Some("Normal"));
    assert_eq!(heading1.next_style.as_deref(), Some("Normal"));
    assert_eq!(heading1.paragraph_properties.keep_next, Some(true));
    assert_eq!(heading1.paragraph_properties.keep_lines, Some(true));
    assert_eq!(heading1.paragraph_properties.spacing_before, Some(240));
    assert_eq!(heading1.paragraph_properties.outline_level, Some(0));
    assert_eq!(heading1.run_properties.bold, Some(true));
    assert_eq!(heading1.run_properties.color.as_deref(), Some("2F5496"));
    assert_eq!(heading1.run_properties.font_size_half_points, Some(32));
    assert_eq!(heading1.run_properties.font_ascii_theme.as_deref(), Some("majorHAnsi"));

    let strong = catalog.styles.get("Strong").expect("Strong style must exist");
    assert_eq!(strong.style_type, StyleType::Character);
    assert_eq!(strong.run_properties.bold, Some(true));
}

#[test]
fn test_resolve_style_inheritance() {
    let xml = make_styles_xml(
        r#"
        <w:docDefaults>
            <w:rPrDefault>
                <w:rPr>
                    <w:sz w:val="24"/>
                    <w:rFonts w:ascii="Calibri"/>
                </w:rPr>
            </w:rPrDefault>
            <w:pPrDefault>
                <w:pPr>
                    <w:spacing w:after="160"/>
                </w:pPr>
            </w:pPrDefault>
        </w:docDefaults>
        <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
            <w:name w:val="Normal"/>
            <w:pPr>
                <w:spacing w:after="200"/>
                <w:jc w:val="left"/>
            </w:pPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Heading1">
            <w:name w:val="heading 1"/>
            <w:basedOn w:val="Normal"/>
            <w:pPr>
                <w:keepNext/>
                <w:spacing w:before="240"/>
                <w:outlineLvl w:val="0"/>
                <w:jc w:val="center"/>
            </w:pPr>
            <w:rPr>
                <w:b/>
                <w:sz w:val="32"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("Heading1");

    assert_eq!(resolved.run_properties.font_ascii.as_deref(), Some("Calibri"));
    assert_eq!(resolved.run_properties.font_size_half_points, Some(32));
    assert_eq!(resolved.run_properties.bold, Some(true));

    assert_eq!(resolved.paragraph_properties.spacing_after, Some(200));
    assert_eq!(resolved.paragraph_properties.alignment.as_deref(), Some("center"));
    assert_eq!(resolved.paragraph_properties.spacing_before, Some(240));
    assert_eq!(resolved.paragraph_properties.keep_next, Some(true));
    assert_eq!(resolved.paragraph_properties.outline_level, Some(0));
}

#[test]
fn test_resolve_style_toggle() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="BoldBase">
            <w:name w:val="Bold Base"/>
            <w:rPr>
                <w:b/>
                <w:i/>
            </w:rPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="NoBold">
            <w:name w:val="No Bold"/>
            <w:basedOn w:val="BoldBase"/>
            <w:rPr>
                <w:b w:val="0"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("NoBold");

    assert_eq!(resolved.run_properties.bold, Some(false));
    assert_eq!(resolved.run_properties.italic, Some(true));
}

#[test]
fn test_resolve_style_cycle_protection() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="StyleA">
            <w:name w:val="Style A"/>
            <w:basedOn w:val="StyleB"/>
            <w:rPr>
                <w:b/>
            </w:rPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="StyleB">
            <w:name w:val="Style B"/>
            <w:basedOn w:val="StyleA"/>
            <w:rPr>
                <w:i/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("StyleA");

    assert_eq!(resolved.run_properties.bold, Some(true));
    assert_eq!(resolved.run_properties.italic, Some(true));
}

#[test]
fn test_resolve_nonexistent_style() {
    let xml = make_styles_xml("");
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("DoesNotExist");

    assert_eq!(resolved, ResolvedStyle::default());
}

#[test]
fn test_resolve_style_with_missing_base() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="Orphan">
            <w:name w:val="Orphan"/>
            <w:basedOn w:val="NonexistentBase"/>
            <w:rPr>
                <w:b/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("Orphan");

    assert_eq!(resolved.run_properties.bold, Some(true));
}

#[test]
fn test_parse_indentation() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="Indented">
            <w:name w:val="Indented"/>
            <w:pPr>
                <w:ind w:left="720" w:right="360" w:firstLine="360" w:hanging="180"/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("Indented").expect("style must exist");
    assert_eq!(style.paragraph_properties.indent_left, Some(720));
    assert_eq!(style.paragraph_properties.indent_right, Some(360));
    assert_eq!(style.paragraph_properties.indent_first_line, Some(360));
    assert_eq!(style.paragraph_properties.indent_hanging, Some(180));
}

#[test]
fn test_parse_underline_none() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="NoUnderline">
            <w:name w:val="No Underline"/>
            <w:rPr>
                <w:u w:val="none"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("NoUnderline").expect("style must exist");
    assert_eq!(style.run_properties.underline, Some(false));
}

#[test]
fn test_parse_underline_single() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="Underlined">
            <w:name w:val="Underlined"/>
            <w:rPr>
                <w:u w:val="single"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("Underlined").expect("style must exist");
    assert_eq!(style.run_properties.underline, Some(true));
}

#[test]
fn test_parse_underline_bare() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="BareUnderline">
            <w:name w:val="Bare Underline"/>
            <w:rPr>
                <w:u/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("BareUnderline").expect("style must exist");
    assert_eq!(style.run_properties.underline, Some(true));
}

#[test]
fn test_parse_bold_explicit_true() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="ExplicitBold">
            <w:name w:val="Explicit Bold"/>
            <w:rPr>
                <w:b w:val="true"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("ExplicitBold").expect("style must exist");
    assert_eq!(style.run_properties.bold, Some(true));
}

#[test]
fn test_parse_invalid_xml() {
    let result = parse_styles_xml("<<<not valid xml");
    assert!(result.is_err());
}

#[test]
fn test_parse_spacing_line_rule() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="SpacingRule">
            <w:name w:val="Spacing Rule"/>
            <w:pPr>
                <w:spacing w:line="360" w:lineRule="exact"/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("SpacingRule").expect("style must exist");
    assert_eq!(style.paragraph_properties.spacing_line, Some(360));
    assert_eq!(style.paragraph_properties.spacing_line_rule.as_deref(), Some("exact"));
}

#[test]
fn test_parse_latent_styles_skipped() {
    let xml = make_styles_xml(
        r#"
        <w:latentStyles w:defLockedState="0" w:defUIPriority="99">
            <w:lsdException w:name="Normal" w:uiPriority="0" w:qFormat="1"/>
        </w:latentStyles>
        <w:style w:type="paragraph" w:styleId="Normal">
            <w:name w:val="Normal"/>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    assert_eq!(catalog.styles.len(), 1);
    assert!(catalog.styles.contains_key("Normal"));
}

#[test]
fn test_parse_vert_align() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="Superscript">
            <w:name w:val="Superscript"/>
            <w:rPr>
                <w:vertAlign w:val="superscript"/>
            </w:rPr>
        </w:style>
        <w:style w:type="character" w:styleId="Subscript">
            <w:name w:val="Subscript"/>
            <w:rPr>
                <w:vertAlign w:val="subscript"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let super_style = catalog.styles.get("Superscript").expect("style must exist");
    assert_eq!(super_style.run_properties.vert_align.as_deref(), Some("superscript"));

    let sub_style = catalog.styles.get("Subscript").expect("style must exist");
    assert_eq!(sub_style.run_properties.vert_align.as_deref(), Some("subscript"));
}

#[test]
fn test_parse_multilingual_fonts() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="MultiLang">
            <w:name w:val="MultiLang"/>
            <w:rPr>
                <w:rFonts w:ascii="Calibri" w:hAnsi="Arial" w:cs="Courier" w:eastAsia="SimSun"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("MultiLang").expect("style must exist");
    assert_eq!(style.run_properties.font_ascii.as_deref(), Some("Calibri"));
    assert_eq!(style.run_properties.font_h_ansi.as_deref(), Some("Arial"));
    assert_eq!(style.run_properties.font_cs.as_deref(), Some("Courier"));
    assert_eq!(style.run_properties.font_east_asia.as_deref(), Some("SimSun"));
}

#[test]
fn test_resolve_character_style() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="BaseChar">
            <w:name w:val="Base Char"/>
            <w:rPr>
                <w:b/>
                <w:sz w:val="24"/>
            </w:rPr>
        </w:style>
        <w:style w:type="character" w:styleId="DerivedChar">
            <w:name w:val="Derived Char"/>
            <w:basedOn w:val="BaseChar"/>
            <w:rPr>
                <w:i/>
                <w:color w:val="FF0000"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("DerivedChar");

    assert_eq!(resolved.run_properties.bold, Some(true));
    assert_eq!(resolved.run_properties.font_size_half_points, Some(24));
    assert_eq!(resolved.run_properties.italic, Some(true));
    assert_eq!(resolved.run_properties.color.as_deref(), Some("FF0000"));
}

#[test]
fn test_deep_inheritance_chain() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="Level0">
            <w:name w:val="Level 0"/>
            <w:rPr>
                <w:sz w:val="20"/>
            </w:rPr>
            <w:pPr>
                <w:spacing w:after="100"/>
            </w:pPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Level1">
            <w:name w:val="Level 1"/>
            <w:basedOn w:val="Level0"/>
            <w:rPr>
                <w:b/>
            </w:rPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Level2">
            <w:name w:val="Level 2"/>
            <w:basedOn w:val="Level1"/>
            <w:rPr>
                <w:i/>
            </w:rPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Level3">
            <w:name w:val="Level 3"/>
            <w:basedOn w:val="Level2"/>
            <w:pPr>
                <w:jc w:val="center"/>
            </w:pPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Level4">
            <w:name w:val="Level 4"/>
            <w:basedOn w:val="Level3"/>
            <w:rPr>
                <w:sz w:val="40"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("Level4");

    assert_eq!(resolved.run_properties.font_size_half_points, Some(40));
    assert_eq!(resolved.run_properties.bold, Some(true));
    assert_eq!(resolved.run_properties.italic, Some(true));
    assert_eq!(resolved.paragraph_properties.alignment.as_deref(), Some("center"));
    assert_eq!(resolved.paragraph_properties.spacing_after, Some(100));
}

#[test]
fn test_parse_page_break_before() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="PageBreakStyle">
            <w:name w:val="Page Break Style"/>
            <w:pPr>
                <w:pageBreakBefore/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("PageBreakStyle").expect("style must exist");
    assert_eq!(style.paragraph_properties.page_break_before, Some(true));
}

#[test]
fn test_parse_widow_control() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="WidowControl">
            <w:name w:val="Widow Control"/>
            <w:pPr>
                <w:widowControl w:val="0"/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("WidowControl").expect("style must exist");
    assert_eq!(style.paragraph_properties.widow_control, Some(false));
}

#[test]
fn test_parse_bidi() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="RightToLeft">
            <w:name w:val="Right to Left"/>
            <w:pPr>
                <w:bidi/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("RightToLeft").expect("style must exist");
    assert_eq!(style.paragraph_properties.bidi, Some(true));
}

#[test]
fn test_parse_shading() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="ShadedPara">
            <w:name w:val="Shaded Paragraph"/>
            <w:pPr>
                <w:shd w:val="clear" w:fill="FFFF00"/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("ShadedPara").expect("style must exist");
    assert_eq!(style.paragraph_properties.shading_val.as_deref(), Some("clear"));
    assert_eq!(style.paragraph_properties.shading_fill.as_deref(), Some("FFFF00"));
}

#[test]
fn test_parse_paragraph_borders() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="BorderedPara">
            <w:name w:val="Bordered Paragraph"/>
            <w:pPr>
                <w:pBdr>
                    <w:top w:val="single"/>
                    <w:bottom w:val="double"/>
                    <w:left w:val="triple"/>
                    <w:right w:val="dashed"/>
                </w:pBdr>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("BorderedPara").expect("style must exist");
    assert_eq!(style.paragraph_properties.border_top.as_deref(), Some("single"));
    assert_eq!(style.paragraph_properties.border_bottom.as_deref(), Some("double"));
    assert_eq!(style.paragraph_properties.border_left.as_deref(), Some("triple"));
    assert_eq!(style.paragraph_properties.border_right.as_deref(), Some("dashed"));
}

#[test]
fn test_paragraph_properties_inheritance() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="paragraph" w:styleId="BaseStyle">
            <w:name w:val="Base Style"/>
            <w:pPr>
                <w:pageBreakBefore/>
                <w:spacing w:after="100"/>
            </w:pPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="DerivedStyle">
            <w:name w:val="Derived Style"/>
            <w:basedOn w:val="BaseStyle"/>
            <w:pPr>
                <w:spacing w:before="50"/>
            </w:pPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("DerivedStyle");

    assert_eq!(resolved.paragraph_properties.page_break_before, Some(true));
    assert_eq!(resolved.paragraph_properties.spacing_before, Some(50));
    assert_eq!(resolved.paragraph_properties.spacing_after, Some(100));
}

#[test]
fn test_parse_highlight() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="HighlightStyle">
            <w:name w:val="Highlight Style"/>
            <w:rPr>
                <w:highlight w:val="yellow"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("HighlightStyle").expect("style must exist");
    assert_eq!(style.run_properties.highlight.as_deref(), Some("yellow"));
}

#[test]
fn test_parse_caps_and_small_caps() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="CapsStyle">
            <w:name w:val="Caps Style"/>
            <w:rPr>
                <w:caps/>
                <w:smallCaps/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("CapsStyle").expect("style must exist");
    assert_eq!(style.run_properties.caps, Some(true));
    assert_eq!(style.run_properties.small_caps, Some(true));
}

#[test]
fn test_parse_text_effects() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="EffectsStyle">
            <w:name w:val="Effects Style"/>
            <w:rPr>
                <w:shadow/>
                <w:outline/>
                <w:emboss/>
                <w:imprint/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("EffectsStyle").expect("style must exist");
    assert_eq!(style.run_properties.shadow, Some(true));
    assert_eq!(style.run_properties.outline, Some(true));
    assert_eq!(style.run_properties.emboss, Some(true));
    assert_eq!(style.run_properties.imprint, Some(true));
}

#[test]
fn test_parse_char_spacing() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="SpacingStyle">
            <w:name w:val="Spacing Style"/>
            <w:rPr>
                <w:spacing w:val="20"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("SpacingStyle").expect("style must exist");
    assert_eq!(style.run_properties.char_spacing, Some(20));
}

#[test]
fn test_parse_theme_color() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="ThemeColorStyle">
            <w:name w:val="Theme Color Style"/>
            <w:rPr>
                <w:color w:val="2F5496" w:themeColor="accent1" w:themeTint="BF" w:themeShade="4D"/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let style = catalog.styles.get("ThemeColorStyle").expect("style must exist");
    assert_eq!(style.run_properties.color.as_deref(), Some("2F5496"));
    assert_eq!(style.run_properties.theme_color.as_deref(), Some("accent1"));
    assert_eq!(style.run_properties.theme_tint.as_deref(), Some("BF"));
    assert_eq!(style.run_properties.theme_shade.as_deref(), Some("4D"));
}

#[test]
fn test_run_properties_inheritance_with_effects() {
    let xml = make_styles_xml(
        r#"
        <w:style w:type="character" w:styleId="BaseCharStyle">
            <w:name w:val="Base Char"/>
            <w:rPr>
                <w:b/>
                <w:caps/>
            </w:rPr>
        </w:style>
        <w:style w:type="character" w:styleId="DerivedCharStyle">
            <w:name w:val="Derived Char"/>
            <w:basedOn w:val="BaseCharStyle"/>
            <w:rPr>
                <w:i/>
                <w:shadow/>
            </w:rPr>
        </w:style>
        "#,
    );
    let catalog = parse_styles_xml(&xml).expect("should parse");

    let resolved = catalog.resolve_style("DerivedCharStyle");

    assert_eq!(resolved.run_properties.bold, Some(true));
    assert_eq!(resolved.run_properties.caps, Some(true));
    assert_eq!(resolved.run_properties.italic, Some(true));
    assert_eq!(resolved.run_properties.shadow, Some(true));
}

#[cfg(test)]
mod numbering_style_tests {
    use super::*;

    /// `w:numPr` inside a style's `w:pPr` is the reference Word writes for its
    /// built-in list styles, so the style parser has to keep it (GH#1663).
    #[test]
    fn a_style_keeps_the_numbering_reference_in_its_paragraph_properties() {
        let xml = r#"<w:pPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:numPr><w:numId w:val="3"/><w:ilvl w:val="2"/></w:numPr>
        </w:pPr>"#;
        let doc = roxmltree::Document::parse(xml).expect("pPr parses");
        let props = parse_paragraph_properties(&doc.root_element());
        assert_eq!(props.numbering_id, Some(3));
        assert_eq!(props.numbering_level, Some(2));
    }

    /// The common shape: a numbering reference with no level on it.
    #[test]
    fn a_style_numbering_reference_without_a_level_leaves_the_level_unset() {
        let xml = r#"<w:pPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:numPr><w:numId w:val="1"/></w:numPr>
        </w:pPr>"#;
        let doc = roxmltree::Document::parse(xml).expect("pPr parses");
        let props = parse_paragraph_properties(&doc.root_element());
        assert_eq!(props.numbering_id, Some(1));
        assert_eq!(props.numbering_level, None);
    }
}
