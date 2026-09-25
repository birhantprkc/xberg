use super::*;

#[test]
fn test_strip_html_tags_simple() {
    let html = "<html><body><p>Hello World</p></body></html>";
    let text = strip_html_tags(html);
    assert!(text.contains("Hello World"));
}

#[test]
fn test_strip_html_tags_with_scripts() {
    let html = "<body><p>Text</p><script>alert('bad');</script><p>More</p></body>";
    let text = strip_html_tags(html);
    assert!(!text.contains("bad"));
    assert!(text.contains("Text"));
    assert!(text.contains("More"));
}

#[test]
fn test_strip_html_tags_with_styles() {
    let html = "<body><p>Text</p><style>.class { color: red; }</style><p>More</p></body>";
    let text = strip_html_tags(html);
    assert!(!text.to_lowercase().contains("color"));
    assert!(text.contains("Text"));
    assert!(text.contains("More"));
}

#[test]
fn test_strip_html_tags_normalizes_whitespace() {
    let html = "<p>Hello   \n\t   World</p>";
    let text = strip_html_tags(html);
    assert!(text.contains("Hello") && text.contains("World"));
}

#[test]
fn test_extract_text_from_xhtml_basic() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><title>Test</title></head>
  <body>
<h1>Chapter One</h1>
<p>This is paragraph text.</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("Chapter One"), "got: {result}");
    assert!(result.contains("This is paragraph text."), "got: {result}");
    assert!(!result.contains("Test"), "head title should be excluded, got: {result}");
}

#[test]
fn test_extract_text_from_xhtml_converts_math_to_latex() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<p>Before</p>
<math xmlns="http://www.w3.org/1998/Math/MathML">
  <mfrac><mn>1</mn><mn>2</mn></mfrac>
</math>
<p>After</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("$$\\frac{1}{2}$$"), "got: {result}");
    assert!(result.contains("Before"), "got: {result}");
    assert!(result.contains("After"), "got: {result}");
    assert!(
        !result.contains("mfrac"),
        "raw MathML tag names must not leak, got: {result}"
    );
    assert!(
        !result.contains("mn"),
        "raw MathML tag names must not leak, got: {result}"
    );
}

#[test]
fn test_extract_text_from_xhtml_budgeted_converts_math_to_latex() {
    let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<math xmlns="http://www.w3.org/1998/Math/MathML"><msup><mi>x</mi><mn>2</mn></msup></math>
  </body>
</html>"#;
    let mut budget = SecurityBudget::from_limits(&crate::extractors::security::SecurityLimits::default());
    let result = extract_text_from_xhtml_budgeted(xhtml, &mut budget);
    assert_eq!(result, "$$x^{2}$$");
}

#[test]
fn test_extract_text_from_xhtml_skips_script_style() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<p>Visible text</p>
<script>var x = 1;</script>
<style>.c { color: red; }</style>
<p>More visible</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("Visible text"), "got: {result}");
    assert!(result.contains("More visible"), "got: {result}");
    assert!(!result.contains("var x"), "got: {result}");
    assert!(!result.contains("color"), "got: {result}");
}

#[test]
fn test_extract_text_from_xhtml_preserves_underscores_and_numbers() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<p>The value_count is 1,000 items worth 3.14 each.</p>
<p>See http://example.com/path_to/resource for details.</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("value_count"), "underscore preserved, got: {result}");
    assert!(result.contains("1,000"), "number preserved, got: {result}");
    assert!(result.contains("3.14"), "decimal preserved, got: {result}");
    assert!(
        result.contains("http://example.com/path_to/resource"),
        "URL preserved, got: {result}"
    );
}

#[test]
fn test_extract_text_from_xhtml_block_elements_add_newlines() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<h1>Heading</h1>
<p>Paragraph one.</p>
<p>Paragraph two.</p>
<ul>
  <li>Item A</li>
  <li>Item B</li>
</ul>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("Heading"), "got: {result}");
    assert!(result.contains("Paragraph one."), "got: {result}");
    assert!(result.contains("Paragraph two."), "got: {result}");
    assert!(result.contains("Item A"), "got: {result}");
    assert!(result.contains("Item B"), "got: {result}");
    assert!(result.contains('\n'), "should have newlines, got: {result}");
}

#[test]
fn test_extract_text_from_xhtml_inline_formatting_preserved() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
<p>This has <strong>bold</strong> and <em>italic</em> text.</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert!(result.contains("bold"), "got: {result}");
    assert!(result.contains("italic"), "got: {result}");
    assert!(!result.contains("**"), "no markdown bold, got: {result}");
    assert!(!result.contains('_'), "no markdown italic, got: {result}");
}

#[test]
fn test_extract_text_from_xhtml_fallback_for_invalid_xml() {
    let bad_xhtml = "<p>Hello <b>World</b> unclosed <p>second";
    let result = extract_text_from_xhtml(bad_xhtml);
    assert!(result.contains("Hello"), "got: {result}");
    assert!(result.contains("World"), "got: {result}");
}

#[test]
fn should_remove_serialized_mathml_comment_and_keep_readable_fallback() {
    let xhtml = r#"<html><body>
<!-- MathML: <math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi><mo>=</mo><mn>2</mn></math> -->
<p>x = 2</p><!-- editorial note -->
</body></html>"#;

    let normalized = normalize_xhtml(xhtml);

    assert!(!normalized.contains("MathML:"), "got: {normalized}");
    assert!(!normalized.contains("<math"), "got: {normalized}");
    assert!(normalized.contains("x = 2"), "got: {normalized}");
    assert!(normalized.contains("<!-- editorial note -->"), "got: {normalized}");
}

#[test]
fn should_remove_embedded_media_without_losing_surrounding_prose() {
    let xhtml = r#"<html><body>
<p>Before</p>
<video><source src="movie.mp4"/><div>Video fallback</div></video>
<audio src="sound.mp3"><p>Audio fallback</p></audio>
<p>After</p>
</body></html>"#;

    let stripped = strip_embedded_media_elements(xhtml);

    assert!(stripped.contains("Before"), "got: {stripped}");
    assert!(stripped.contains("After"), "got: {stripped}");
    assert!(!stripped.contains("movie.mp4"), "got: {stripped}");
    assert!(!stripped.contains("sound.mp3"), "got: {stripped}");
    assert!(!stripped.contains("fallback"), "got: {stripped}");
}

#[test]
fn should_resolve_epub_switch_to_supported_case_or_default() {
    let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
<body>
<epub:switch xmlns:epub="http://www.idpf.org/2007/ops">
  <epub:case required-namespace="urn:unsupported"><p>UNKNOWN_CASE</p></epub:case>
  <epub:default><p>DEFAULT</p></epub:default>
</epub:switch>
<epub:switch xmlns:epub="http://www.idpf.org/2007/ops">
  <epub:case required-namespace="http://www.w3.org/1998/Math/MathML">
<math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math>
  </epub:case>
  <epub:default><p>MATH_FALLBACK</p></epub:default>
</epub:switch>
<epub:switch xmlns:epub="http://www.idpf.org/2007/ops">
  <epub:case required-namespace="http://www.w3.org/1999/xhtml">
<p>XHTML_CASE</p>
<epub:switch>
  <epub:case required-namespace="urn:nested-unsupported"><p>NESTED_WRONG</p></epub:case>
  <epub:default><p>NESTED_DEFAULT</p></epub:default>
</epub:switch>
  </epub:case>
  <epub:default><p>XHTML_FALLBACK</p></epub:default>
</epub:switch>
<switch><p>ORDINARY</p></switch>
</body></html>"#;

    let markup = resolve_epub_switch_elements(xhtml, &[XHTML_NAMESPACE, MATHML_NAMESPACE]);
    let plain = resolve_epub_switch_elements(xhtml, &[XHTML_NAMESPACE]);

    for resolved in [&markup, &plain] {
        assert!(resolved.contains("DEFAULT"), "got: {resolved}");
        assert!(resolved.contains("XHTML_CASE"), "got: {resolved}");
        assert!(resolved.contains("NESTED_DEFAULT"), "got: {resolved}");
        assert!(resolved.contains("<switch><p>ORDINARY</p></switch>"), "got: {resolved}");
        assert!(!resolved.contains("UNKNOWN_CASE"), "got: {resolved}");
        assert!(!resolved.contains("NESTED_WRONG"), "got: {resolved}");
        assert!(!resolved.contains("XHTML_FALLBACK"), "got: {resolved}");
        assert!(roxmltree::Document::parse(resolved).is_ok(), "got: {resolved}");
    }
    assert!(markup.contains("<mi>x</mi>"), "got: {markup}");
    assert!(!markup.contains("MATH_FALLBACK"), "got: {markup}");
    assert!(!plain.contains("<mi>x</mi>"), "got: {plain}");
    assert!(plain.contains("MATH_FALLBACK"), "got: {plain}");
}

#[test]
fn test_normalise_inline_whitespace() {
    assert_eq!(normalise_inline_whitespace("hello   world"), "hello world");
    assert_eq!(normalise_inline_whitespace("  leading"), " leading");
    assert_eq!(normalise_inline_whitespace("trailing  "), "trailing ");
    assert_eq!(normalise_inline_whitespace("a\n\t b"), "a b");
}

#[test]
fn test_collapse_blank_lines() {
    let input = "a\n\n\n\nb";
    let result = collapse_blank_lines(input);
    assert_eq!(result, "a\n\nb");

    let input2 = "a\n\nb";
    assert_eq!(collapse_blank_lines(input2), "a\n\nb");
}

#[test]
fn test_expand_named_entities_replaces_html_references_and_keeps_xml_ones() {
    let xhtml = "<p>A&nbsp;B &mdash; C &amp; D &lt;E&gt; &#169; &unknown; &</p>";
    let expanded = expand_named_entities(xhtml);
    assert_eq!(
        expanded,
        "<p>A\u{a0}B \u{2014} C &amp; D &lt;E&gt; &#169; &unknown; &</p>"
    );
}

#[test]
fn test_extract_text_from_xhtml_with_html_entities_keeps_structure() {
    let xhtml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><link rel="stylesheet" href="s.css"/><title></title></head>
  <body>
<p style="text-indent:0">First&nbsp;para</p>
<p>Second &eacute;</p>
  </body>
</html>"#;
    let result = extract_text_from_xhtml(xhtml);
    assert_eq!(result, "First\u{a0}para\nSecond \u{e9}", "got: {result}");
}

#[test]
fn test_strip_xml_prelude_removes_a_byte_order_mark() {
    let xhtml = "\u{FEFF}<?xml version=\"1.0\"?><!DOCTYPE html><html><body><p>Bom body</p></body></html>";
    let result = extract_text_from_xhtml(xhtml);
    assert_eq!(result, "Bom body", "got: {result}");
}

#[test]
fn test_strip_html_tags_only_skips_real_script_and_style_elements() {
    let html = r#"<body><link rel="stylesheet" href="s.css"/><p style="x">Keep &amp; <span class="subscript">this</span></p> <noscript>also</noscript> <style>p{}</style> <p>end</p>"#;
    let text = strip_html_tags(html);
    assert_eq!(text, "Keep & this also end", "got: {text}");
}

#[test]
fn test_deeply_nested_xhtml_does_not_overflow_the_stack() {
    let depth = 50_000;
    let mut xhtml = String::from("<html><body><p>lead</p>");
    for _ in 0..depth {
        xhtml.push_str("<span>");
    }
    xhtml.push_str("deep");
    for _ in 0..depth {
        xhtml.push_str("</span>");
    }
    xhtml.push_str("<p>tail</p></body></html>");

    let text = extract_text_from_xhtml(&xhtml);
    assert!(text.contains("lead"), "got: {text}");
    assert!(text.contains("tail"), "got: {text}");

    let mut budget = SecurityBudget::from_limits(&crate::extractors::security::SecurityLimits::default());
    let text = extract_text_from_xhtml_budgeted(&xhtml, &mut budget);
    assert!(text.contains("lead"), "got: {text}");
    assert!(text.contains("tail"), "got: {text}");
}

#[test]
fn test_deeply_nested_svg_does_not_overflow_the_stack() {
    let depth = 50_000;
    let mut xhtml = String::from("<html><body><svg>");
    for _ in 0..depth {
        xhtml.push_str("<g>");
    }
    xhtml.push_str("<text>deep</text>");
    for _ in 0..depth {
        xhtml.push_str("</g>");
    }
    xhtml.push_str("</svg><p>tail</p></body></html>");

    let text = extract_text_from_xhtml(&xhtml);
    assert!(text.contains("tail"), "got: {text}");
}
