use super::super::*;

#[test]
fn test_garbled_text() {
    let md = "\
# Title

\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}

Normal text here.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        !garbled.is_empty(),
        "Expected garbled text detection for mojibake / replacement characters"
    );
}

#[test]
fn test_german_umlauts_not_flagged_as_garbled() {
    let md = "\
# Title

\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}\u{00e4}\u{00f6}\u{00fc}

Normal text here.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "German umlauts should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_table_separator_not_flagged_as_garbled() {
    let md = "\
| Col1 | Col2 |
|------|------|
| a    | b    |
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Table separator rows should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_horizontal_rule_not_flagged_as_garbled() {
    let md = "\
# Section

---

More text.

***

Even more text.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Horizontal rules should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_arabic_text_not_flagged_as_garbled() {
    let md = "\
# Document

Some English text mixed with \u{0645}\u{0631}\u{062d}\u{0628}\u{0627} Arabic words in a sentence.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Mixed Arabic/English text below 70% non-ASCII should not be flagged, got: {:?}",
        garbled
    );
}

#[test]
fn test_truly_garbled_text_still_flagged() {
    let md = "\
# Title

\u{FFFD}\u{E000}\u{FFFD}\u{E001}\u{FFFD}\u{E002}\u{FFFD}\u{E003}\u{FFFD}\u{E004}\u{FFFD}\u{E005}\u{FFFD}\u{E006}\u{FFFD}\u{E007}\u{FFFD}\u{E008}\u{FFFD}\u{E009}

Normal text here.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        !garbled.is_empty(),
        "Truly garbled text (Private Use Area / replacement chars) should still be flagged"
    );
}

#[test]
fn test_markdown_structural_punct_not_flagged_as_garbled() {
    let md = "\
# Title

Some text with --- dashes in it.

Text with **bold** and ~~strike~~ formatting.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Markdown structural punctuation should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_empty_image_link_not_flagged_as_garbled() {
    let md = "\
# Gallery

![](image1.png)
![]()
![alt text](http://example.com/img.jpg)

Normal text.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Empty image links ![]() should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_escaped_markdown_links_not_flagged_as_garbled() {
    let md = "\
# Wikipedia Article

\\[Big Machine Records\\](/wiki/Big_Machine_Records)
\\[Taylor Swift\\](/wiki/Taylor_Swift)

Normal text here.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Escaped markdown links should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_toc_dot_leaders_not_flagged_as_garbled() {
    let md = "\
# Table of Contents

Foreword .............. v
Chapter 1 ............ 1
Chapter 2 ............ 15
Appendix ............. 200
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "TOC dot leaders should not be flagged as garbled text, got: {:?}",
        garbled
    );
}

#[test]
fn test_truly_garbled_punct_still_flagged() {
    let md = "\
# Title

Some text with @@@@garbled content here.

Normal text.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        !garbled.is_empty(),
        "Non-structural consecutive punctuation (@@@@) should still be flagged as garbled text"
    );
}

#[test]
fn test_pure_arabic_text_not_flagged_as_garbled() {
    let md = "\
# Document

\u{062a}\u{062d}\u{0633}\u{064a}\u{0646} \u{0627}\u{0644}\u{0625}\u{0646}\u{062a}\u{0627}\u{062c}\u{064a}\u{0629} \u{0648}\u{062d}\u{0644} \u{0627}\u{0644}\u{0645}\u{0634}\u{0643}\u{0644}\u{0627}\u{062a}
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Pure Arabic text should not be flagged as garbled, got: {:?}",
        garbled
    );
}

#[test]
fn test_pure_chinese_text_not_flagged_as_garbled() {
    let md = "\
# Document

\u{80a1}\u{7968}\u{4ee3}\u{7801}\u{ff1a}\u{4e09}\u{96f6}\u{4e8c}\u{4e00}\u{516b} \u{80a1}\u{7968}\u{7b80}\u{79f0}\u{ff1a}\u{5b89}\u{5229}\u{80a1}\u{4efd}
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Pure Chinese text should not be flagged as garbled, got: {:?}",
        garbled
    );
}

#[test]
fn test_pure_cyrillic_text_not_flagged_as_garbled() {
    let md = "\
# Document

\u{0420}\u{0430}\u{0437}\u{0434}\u{0435}\u{043b}\u{003a} \u{0424}\u{0438}\u{043d}\u{0430}\u{043b}\u{044c}\u{043d}\u{044b}\u{0439} \u{044d}\u{043a}\u{0437}\u{0430}\u{043c}\u{0435}\u{043d}
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Pure Cyrillic text should not be flagged as garbled, got: {:?}",
        garbled
    );
}

#[test]
fn test_math_operators_not_flagged_as_garbled() {
    let md = "\
# Math

\u{2211}\u{2208}\u{2209}\u{221A}\u{221E}\u{2264}\u{2265}\u{2260}\u{2261}\u{2248}\u{2202}\u{222B}\u{220F}\u{2200}\u{2203}\u{2207}\u{2211}\u{2208}\u{2209}\u{221A}
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "Mathematical operators should not be flagged as garbled, got: {:?}",
        garbled
    );
}

#[test]
fn test_latex_braces_not_flagged_as_garbled() {
    let md = "\
# Equation

The formula is \\sum_{k=0}^{n} C{k}{n} x^{k}.
";
    let report = detect_noise(md);
    let garbled: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == NoiseKind::GarbledText)
        .collect();
    assert!(
        garbled.is_empty(),
        "LaTeX brace patterns should not be flagged as garbled, got: {:?}",
        garbled
    );
}
