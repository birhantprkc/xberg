use super::super::*;

#[test]
fn test_broken_table() {
    let md = "\
| Col1 | Col2 | Col3 |
|------|------|------|
| a | b | c |
| d | e |
";
    let report = detect_noise(md);
    let table_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::BrokenTable)
        .collect();
    assert!(!table_issues.is_empty(), "Expected broken table issues");
    assert_eq!(table_issues[0].severity, Severity::Warning);
}

#[test]
fn test_table_header_words_excluded() {
    let mut lines = Vec::new();
    for i in 0..15 {
        lines.push("Test Case Identifier".to_string());
        for j in 0..4 {
            lines.push(format!("Content line {j} of section {i} with text."));
        }
    }
    let md = lines.join("\n");
    let report = detect_noise(&md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "Title Case table headers under 40 chars should not be flagged, got {} issues",
        rep_issues.len()
    );
}

#[test]
fn test_escaped_pipes_not_counted_in_broken_table() {
    let md = "\
| Col1 | Col2 | Col3 |
|------|------|------|
| a\\|b | c | d |
| e | f\\|g | h |
";
    let report = detect_noise(md);
    let table_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::BrokenTable)
        .collect();
    assert!(
        table_issues.is_empty(),
        "Escaped pipes in table cells should not cause BrokenTable, got: {:?}",
        table_issues
    );
}
