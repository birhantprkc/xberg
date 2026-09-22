use super::super::*;

#[test]
fn test_pipe_table_rows_not_flagged_as_header_footer() {
    let md = "\
| Col1 | Col2 | Col3 |
|------|------|------|
|  |  |  |
|  |  |  |
|  |  |  |
|  |  |  |
|  |  |  |
|  |  |  |
";
    let report = detect_noise(md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "Pipe table rows should not be flagged as header/footer repetition, got: {:?}",
        rep_issues
    );
}

#[test]
fn test_image_placeholders_not_flagged_as_header_footer() {
    let md = "\
# Gallery

![](image1.png)
![](image2.png)
![](image3.png)
![](image4.png)
![](image5.png)
![](image6.png)
";
    let report = detect_noise(md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "Image placeholders should not be flagged as header/footer repetition, got: {:?}",
        rep_issues
    );
}

#[test]
fn test_short_repeated_lines_not_flagged_as_header_footer() {
    let md = "\
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text
Hello world text

Some other text here.
";
    let report = detect_noise(md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "Short repeated lines (< 20 non-ws chars) should not be flagged, got: {:?}",
        rep_issues
    );
}

#[test]
fn test_genuine_header_footer_repetition_still_flagged() {
    let md = "\
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 1
Some content here.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 2
More content here.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 3
Even more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 4
Still more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 5
Additional content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 6
Further content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 7
Yet more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 8
Content eight.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 9
Content nine.
Copyright 2024 Acme Corporation All Rights Reserved
";
    let report = detect_noise(md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        !rep_issues.is_empty(),
        "Genuine header/footer repetition (10+ times, periodic) should be flagged"
    );
    assert_eq!(rep_issues.len(), 10);
}

#[test]
fn test_nine_repetitions_no_longer_flagged() {
    let md = "\
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 1
Some content here.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 2
More content here.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 3
Even more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 4
Still more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 5
Additional content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 6
Further content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 7
Yet more content.
Copyright 2024 Acme Corporation All Rights Reserved
# Chapter 8
Content eight.
Copyright 2024 Acme Corporation All Rights Reserved
";
    let report = detect_noise(md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "9 repetitions should not be flagged (threshold is 10), got: {:?}",
        rep_issues
    );
}

#[test]
fn test_iso_column_headers_not_flagged_as_header_footer() {
    let mut lines = Vec::new();
    for i in 0..200 {
        lines.push(format!("## Test Case {i}"));
        lines.push("Item Content Description".to_string());
        lines.push("Prerequisite Condition".to_string());
        lines.push("Expected Test Result".to_string());
        if i % 3 == 0 {
            lines.push("Additional Notes Section".to_string());
            lines.push(String::new());
        }
        lines.push(format!("Test step {i}: verify the output is correct."));
        lines.push(String::new());
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
        "ISO-style column headers at irregular intervals should not be flagged, got {} issues",
        rep_issues.len()
    );
}

#[test]
fn test_periodic_real_headers_are_flagged() {
    let mut lines = Vec::new();
    for page in 0..12 {
        lines.push("ACME Corporation - Internal Document - Confidential Draft".to_string());
        for j in 0..49 {
            lines.push(format!("Content line {j} of page {page} with enough text."));
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
        !rep_issues.is_empty(),
        "Periodic real headers (every ~50 lines, 12 occurrences) should be flagged"
    );
    assert_eq!(rep_issues.len(), 12);
}

#[test]
fn test_header_footer_cap_at_30_issues() {
    let mut lines = Vec::new();
    for page in 0..50 {
        lines.push("ACME Corporation - Internal Document - Confidential Draft".to_string());
        for j in 0..49 {
            lines.push(format!("Content line {j} of page {page} with enough text."));
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
        rep_issues.len() <= 30,
        "Header/footer issues should be capped at 30, got {}",
        rep_issues.len()
    );
}

#[test]
fn test_irregular_repetition_not_flagged() {
    let mut lines = Vec::new();
    let repeated = "This particular content line appears many times in the document";
    let positions = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 50, 200];
    let total_lines = 300;
    for i in 0..total_lines {
        if positions.contains(&i) {
            lines.push(repeated.to_string());
        } else {
            lines.push(format!("Regular content line number {i} in the document."));
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
        "Irregular (non-periodic) repetition should not be flagged, got {} issues",
        rep_issues.len()
    );
}

#[test]
fn test_rst_grid_table_borders_not_flagged_as_header_footer() {
    let mut lines = Vec::new();
    for i in 0..15 {
        lines.push("+-----+-----+-----+".to_string());
        lines.push(format!("| a{i}  | b{i}  | c{i}  |"));
    }
    lines.push("+-----+-----+-----+".to_string());
    let md = lines.join("\n");
    let report = detect_noise(&md);
    let rep_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::HeaderFooterRepetition)
        .collect();
    assert!(
        rep_issues.is_empty(),
        "RST grid table borders should not be flagged as header/footer repetition, got: {:?}",
        rep_issues
    );
}
