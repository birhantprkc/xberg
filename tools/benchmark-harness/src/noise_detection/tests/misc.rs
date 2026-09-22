use super::super::*;

#[test]
fn test_clean_markdown() {
    let md = "\
# Hello World

This is a paragraph with some text.

## Section Two

Another paragraph here with more content.

- Item one
- Item two
- Item three
";
    let report = detect_noise(md);
    assert!(
        report.issues.is_empty(),
        "Expected 0 issues for clean markdown, got: {:?}",
        report.issues
    );
    assert_eq!(report.summary.noise_score, 0.0);
}

#[test]
fn test_empty_heading() {
    let md = "\
#

Some content here.
";
    let report = detect_noise(md);
    let heading_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::EmptyHeading)
        .collect();
    assert_eq!(heading_issues.len(), 1);
    assert_eq!(heading_issues[0].severity, Severity::Error);
    assert_eq!(heading_issues[0].line, 1);
}

#[test]
fn test_code_block_skipped() {
    let md = "\
# Title

```html
<div>This should not be flagged</div>
<table><tr><td>Also not flagged</td></tr></table>
```

Normal paragraph.
";
    let report = detect_noise(md);
    let html_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HtmlRemnant)
        .collect();
    assert!(
        html_issues.is_empty(),
        "HTML inside code blocks should not be flagged, got: {:?}",
        html_issues
    );
}

#[test]
fn test_page_numbers() {
    let mut lines = vec!["# Title".to_string(), String::new()];
    for page in 1..=6 {
        lines.push(format!("Page {page} content with enough text to fill the space."));
        lines.push(String::new());
        lines.push(String::new());
        lines.push(String::new());
        lines.push(page.to_string());
        lines.push(String::new());
    }
    lines.push("Final text.".to_string());
    let md = lines.join("\n");
    let report = detect_noise(&md);
    let page_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::PageNumberArtifact)
        .collect();
    assert!(
        !page_issues.is_empty(),
        "Expected page number artifact detection for sequential standalone numbers"
    );
    assert_eq!(page_issues.len(), 6);
}

#[test]
fn test_clustered_numbers_not_flagged_as_page_numbers() {
    let md = "\
# Table Data

1
2
3
4
5
6
7

Some text after.
";
    let report = detect_noise(md);
    let page_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::PageNumberArtifact)
        .collect();
    assert!(
        page_issues.is_empty(),
        "Clustered sequential numbers (table data) should not be flagged as page numbers, got: {:?}",
        page_issues
    );
}

#[test]
fn test_dangling_footnote() {
    let md = "\
# Title

This has a reference[^1] and another[^2].

[^1]: This is defined.
";
    let report = detect_noise(md);
    let dangling: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::DanglingReference)
        .collect();
    assert!(!dangling.is_empty(), "Expected dangling reference for [^2]");
    assert!(
        dangling.iter().all(|i| {
            let line = &md.lines().collect::<Vec<_>>()[i.line - 1];
            line.contains("[^2]")
        }),
        "Only [^2] should be flagged as dangling"
    );
}

#[test]
fn test_empty_input() {
    let report = detect_noise("");
    assert!(report.issues.is_empty());
    assert_eq!(report.summary.total_issues, 0);
    assert_eq!(report.summary.noise_score, 0.0);
}

#[test]
fn test_cjk_text_below_threshold_not_flagged() {
    let md = "\
# Document

This line has some CJK chars \u{4f60}\u{597d} mixed with English text here.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Mixed CJK/English text below 70% non-ASCII should not be flagged, got: {:?}",
        garbled
    );
}

#[test]
fn test_excessive_heading_density_raised_threshold() {
    let md = "\
# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6
# Heading 7
## Heading 8

Paragraph one.

Paragraph two.

Paragraph three.
";
    let report = detect_noise(md);
    let density_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::ExcessiveHeadingDensity)
        .collect();
    assert!(
        density_issues.is_empty(),
        "8 headings vs 3 paragraphs should not be flagged with raised threshold, got: {:?}",
        density_issues
    );
}
