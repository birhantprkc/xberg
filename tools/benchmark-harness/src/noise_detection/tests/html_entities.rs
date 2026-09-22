use super::super::*;

#[test]
fn test_html_remnant_detection() {
    let md = "\
# Title

<div class=\"content\">Some text</div>

More text here.
";
    let report = detect_noise(md);
    let html_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HtmlRemnant)
        .collect();
    assert!(!html_issues.is_empty(), "Expected HTML remnant issues");
    assert_eq!(html_issues[0].severity, Severity::Warning);
}

#[test]
fn test_numeric_html_entity_detected() {
    let md = "\
# Document

This has an unresolved entity&#10;in the middle.

Normal text.
";
    let report = detect_noise(md);
    let entity_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::UnresolvedHtmlEntity)
        .collect();
    assert!(
        !entity_issues.is_empty(),
        "Numeric HTML entity &#10; should be detected as UnresolvedHtmlEntity"
    );
}

#[test]
fn test_named_html_entity_detected() {
    let md = "\
# Document

This has &amp; and &nbsp; entities.

Normal text.
";
    let report = detect_noise(md);
    let entity_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::UnresolvedHtmlEntity)
        .collect();
    assert!(
        !entity_issues.is_empty(),
        "Named HTML entities &amp; and &nbsp; should be detected"
    );
}

#[test]
fn test_html_entity_in_code_block_not_detected() {
    let md = "\
# Document

```html
This has &amp; entities in code.
```

Normal text.
";
    let report = detect_noise(md);
    let entity_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::UnresolvedHtmlEntity)
        .collect();
    assert!(
        entity_issues.is_empty(),
        "HTML entities inside code blocks should not be flagged, got: {:?}",
        entity_issues
    );
}

#[test]
fn test_escaped_html_tags_not_flagged() {
    let md = "\
# Wikipedia Infobox

| Traded as | \\<br\\>Nasdaq: MSFT |
|---|---|
| Brands | \\<br\\>Windows, Xbox |
";
    let report = detect_noise(md);
    let html_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HtmlRemnant)
        .collect();
    assert!(
        html_issues.is_empty(),
        "Backslash-escaped HTML tags should not be flagged, got: {:?}",
        html_issues
    );
}

#[test]
fn test_real_html_still_flagged_alongside_escaped() {
    let md = "\
# Title

\\<br\\> but also <div>real html</div> here.
";
    let report = detect_noise(md);
    let html_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HtmlRemnant)
        .collect();
    assert!(
        !html_issues.is_empty(),
        "Line with real (non-escaped) HTML tags should still be flagged"
    );
}

#[test]
fn test_indented_code_block_html_not_flagged() {
    let md = "\
# Title

Here's a code example:

    <div>
        <p>This is inside an indented code block</p>
    </div>

Normal paragraph after.
";
    let report = detect_noise(md);
    let html_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HtmlRemnant)
        .collect();
    assert!(
        html_issues.is_empty(),
        "HTML inside indented code blocks should not be flagged, got: {:?}",
        html_issues
    );
}
