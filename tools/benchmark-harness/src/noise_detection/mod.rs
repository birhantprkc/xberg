//! Noise and dirt detection for markdown extraction output.
//!
//! Detects common quality issues in extracted markdown such as HTML remnants,
//! garbled text, broken tables, page number artifacts, and other extraction
//! artifacts. All heuristics operate on the raw markdown string, line by line,
//! skipping content inside fenced code blocks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single noise issue found in the markdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseIssue {
    /// The kind of noise detected.
    pub kind: NoiseKind,
    /// 1-indexed line number where the issue was found.
    pub line: usize,
    /// ~80 char preview of the offending line.
    pub context: String,
    /// Severity of the issue.
    pub severity: Severity,
}

/// Categories of noise that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoiseKind {
    /// HTML tags found outside code blocks.
    HtmlRemnant,
    /// Runs of 4+ consecutive blank lines.
    ExcessiveWhitespace,
    /// Lines with high non-ASCII ratio or consecutive punctuation.
    GarbledText,
    /// Heading markers with no content text.
    EmptyHeading,
    /// Pipe tables with inconsistent column counts.
    BrokenTable,
    /// List markers (`-`, `*`, `+`, `1.`) with no content.
    OrphanedListMarker,
    /// Standalone small numbers that look like page numbers.
    PageNumberArtifact,
    /// Lines repeated 10+ times at regular intervals in the document.
    HeaderFooterRepetition,
    /// Footnote references without matching definitions.
    DanglingReference,
    /// More headings than paragraphs (heading-heavy document).
    ExcessiveHeadingDensity,
    /// Unresolved HTML entities like `&#10;` or `&amp;` outside code blocks.
    UnresolvedHtmlEntity,
}

impl NoiseKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::HtmlRemnant => "HtmlRemnant",
            Self::ExcessiveWhitespace => "ExcessiveWhitespace",
            Self::GarbledText => "GarbledText",
            Self::EmptyHeading => "EmptyHeading",
            Self::BrokenTable => "BrokenTable",
            Self::OrphanedListMarker => "OrphanedListMarker",
            Self::PageNumberArtifact => "PageNumberArtifact",
            Self::HeaderFooterRepetition => "HeaderFooterRepetition",
            Self::DanglingReference => "DanglingReference",
            Self::ExcessiveHeadingDensity => "ExcessiveHeadingDensity",
            Self::UnresolvedHtmlEntity => "UnresolvedHtmlEntity",
        }
    }
}

/// Severity levels for noise issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Informational — minor cosmetic issues.
    Info,
    /// Warning — likely extraction artifacts.
    Warning,
    /// Error — definite extraction failures.
    Error,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }
}

/// Full diagnostic report for a markdown document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// All noise issues found.
    pub issues: Vec<NoiseIssue>,
    /// Aggregated summary.
    pub summary: NoiseSummary,
}

/// Aggregated summary of noise issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseSummary {
    /// Total number of issues found.
    pub total_issues: usize,
    /// Issue counts grouped by kind.
    pub by_kind: HashMap<String, usize>,
    /// Issue counts grouped by severity.
    pub by_severity: HashMap<String, usize>,
    /// Overall noise score: 0.0 = clean, 1.0 = extremely noisy.
    pub noise_score: f64,
}

/// Represents a range of lines inside a fenced code block.
#[derive(Debug, Clone, Copy)]
struct CodeRange {
    start: usize,
    end: usize,
}

/// Returns true if the given 0-indexed line is inside any code range.
fn in_code_block(line_idx: usize, code_ranges: &[CodeRange]) -> bool {
    code_ranges.iter().any(|r| line_idx >= r.start && line_idx <= r.end)
}

/// Identifies fenced code block ranges (``` or ~~~) using a simple state machine.
/// Also detects indented code blocks (4+ space or 1+ tab indentation on
/// consecutive lines, preceded by a blank line).
fn find_code_ranges(lines: &[&str]) -> Vec<CodeRange> {
    let mut ranges = Vec::new();
    let mut in_fence = false;
    let mut fence_start = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if in_fence {
                ranges.push(CodeRange {
                    start: fence_start,
                    end: i,
                });
                in_fence = false;
            } else {
                fence_start = i;
                in_fence = true;
            }
        }
    }

    let mut i = 0;
    while i < lines.len() {
        if in_code_block(i, &ranges) {
            i += 1;
            continue;
        }

        let is_indented_code = lines[i].starts_with("    ") || lines[i].starts_with('\t');
        if is_indented_code {
            let preceded_by_blank = i == 0 || lines[i - 1].trim().is_empty();
            if preceded_by_blank {
                let block_start = i;
                while i < lines.len()
                    && (lines[i].starts_with("    ") || lines[i].starts_with('\t') || lines[i].trim().is_empty())
                {
                    i += 1;
                }
                let block_end = i.saturating_sub(1);
                ranges.push(CodeRange {
                    start: block_start,
                    end: block_end,
                });
                continue;
            }
        }
        i += 1;
    }

    ranges
}

/// Truncates a string to approximately `max_len` characters for context previews.
/// Uses char boundaries to avoid panicking on multi-byte UTF-8 sequences.
fn truncate_context(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max_len);
        format!("{}...", &s[..end])
    }
}

/// Returns true if ALL occurrences of `tag` in `lower` are preceded by a backslash,
/// indicating they are markdown-escaped and not real HTML.
fn all_tag_occurrences_escaped(lower: &str, tag: &str) -> bool {
    let mut start = 0;
    let mut found_any = false;
    while let Some(pos) = lower[start..].find(tag) {
        let abs_pos = start + pos;
        found_any = true;
        if abs_pos == 0 || lower.as_bytes()[abs_pos - 1] != b'\\' {
            return false;
        }
        start = abs_pos + tag.len();
    }
    found_any
}

/// Detects HTML tags outside code blocks.
///
/// Skips tags that are backslash-escaped (e.g., `\<br\>`) since those are
/// literal text in markdown, not actual HTML remnants.
fn detect_html_remnants(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let html_tags = [
        "<table", "</table", "<tr", "</tr", "<td", "</td", "<th", "</th", "<div", "</div", "<span", "</span", "<p>",
        "</p>", "<p ", "<br", "<b>", "</b>", "<strong", "</strong", "<i>", "</i>", "<em", "</em", "<a ", "</a>", "<a>",
        "<img", "<pre", "</pre", "<code", "</code", "<ul", "</ul", "<ol", "</ol", "<li", "</li", "<h1", "</h1", "<h2",
        "</h2", "<h3", "</h3", "<h4", "</h4", "<h5", "</h5", "<h6", "</h6", "<sup", "</sup", "<sub", "</sub",
    ];

    let mut issues = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let lower = line.to_lowercase();
        for tag in &html_tags {
            if lower.contains(tag) && !all_tag_occurrences_escaped(&lower, tag) {
                issues.push(NoiseIssue {
                    kind: NoiseKind::HtmlRemnant,
                    line: i + 1,
                    context: truncate_context(line, 80),
                    severity: Severity::Warning,
                });
                break;
            }
        }
    }
    issues
}

/// Detects runs of 4+ consecutive blank lines.
fn detect_excessive_whitespace(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();
    let mut blank_run_start: Option<usize> = None;
    let mut blank_count = 0;

    let flush_blank_run = |issues: &mut Vec<NoiseIssue>, count: usize, run_start: Option<usize>| {
        if let Some(start) = run_start
            && count >= 4
        {
            issues.push(NoiseIssue {
                kind: NoiseKind::ExcessiveWhitespace,
                line: start + 1,
                context: format!("{count} consecutive blank lines"),
                severity: Severity::Info,
            });
        }
    };

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            flush_blank_run(&mut issues, blank_count, blank_run_start);
            blank_count = 0;
            blank_run_start = None;
            continue;
        }

        if line.trim().is_empty() {
            if blank_run_start.is_none() {
                blank_run_start = Some(i);
            }
            blank_count += 1;
        } else {
            flush_blank_run(&mut issues, blank_count, blank_run_start);
            blank_count = 0;
            blank_run_start = None;
        }
    }

    flush_blank_run(&mut issues, blank_count, blank_run_start);

    issues
}

/// Returns true if the line is a markdown table separator row (e.g., `|---|---|`).
fn is_table_separator_row(trimmed: &str) -> bool {
    trimmed.starts_with('|') && trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
}

/// Returns true if the line is a markdown horizontal rule (`---`, `***`, `===`, `___`).
fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap_or(' ');
    matches!(first, '-' | '*' | '=' | '_') && trimmed.chars().all(|c| c == first || c == ' ')
}

/// Characters that commonly appear in markdown structural punctuation and should
/// NOT trigger the consecutive-punctuation garbled-text heuristic.
/// Covers both block-level (`-`, `|`, `*`, etc.) and inline syntax (`!`, `[`, `]`,
/// `(`, `)`, `\`, `.`, `/`) plus HTML entity delimiters (`&`, `;`).
const MARKDOWN_STRUCTURAL_PUNCT: &[char] = &[
    '-', '|', '*', '_', '=', '~', ':', '#', '>', '.', '/', '!', '[', ']', '(', ')', '\\', '{', '}', '&', ';', '\'',
    '"', '+',
];

/// Returns true if the character belongs to a recognized non-Latin script that
/// commonly appears in multilingual documents: Arabic, CJK, Cyrillic, Greek,
/// Hebrew, Devanagari, Thai, Korean Hangul, Japanese Kana, etc.
///
/// This is intentionally broad to avoid flagging legitimate multilingual content.
fn is_known_script_char(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x00C0..=0x024F |
        0x0370..=0x03FF |
        0x0400..=0x04FF |
        0x0530..=0x058F |
        0x0590..=0x05FF |
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF |
        0x0900..=0x097F |
        0x0980..=0x0DFF |
        0x0E00..=0x0E7F |
        0x10A0..=0x10FF |
        0x1100..=0x11FF |
        0x2000..=0x206F |
        0x2070..=0x209F |
        0x20A0..=0x20CF |
        0x2100..=0x214F |
        0x2150..=0x218F |
        0x2190..=0x21FF |
        0x2200..=0x22FF |
        0x2460..=0x24FF |
        0x25A0..=0x25FF |
        0x2600..=0x26FF |
        0x2700..=0x27BF |
        0x2E80..=0x9FFF |
        0xAC00..=0xD7AF |
        0xF900..=0xFAFF |
        0xFB50..=0xFDFF | 0xFE70..=0xFEFF |
        0x20000..=0x2FA1F
    )
}

/// Returns true if the non-ASCII characters on this line are predominantly from
/// recognized scripts (not mojibake / encoding errors).
fn is_legitimate_multilingual(non_ws_chars: &[char]) -> bool {
    let non_ascii_chars: Vec<char> = non_ws_chars.iter().copied().filter(|c| !c.is_ascii()).collect();
    if non_ascii_chars.is_empty() {
        return true;
    }
    let known_count = non_ascii_chars.iter().filter(|c| is_known_script_char(**c)).count();
    (known_count as f64 / non_ascii_chars.len() as f64) >= 0.8
}

/// Detects garbled text: lines with >70% non-ASCII (unless from known scripts)
/// or 4+ consecutive non-structural punctuation.
fn detect_garbled_text(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_table_separator_row(trimmed) || is_horizontal_rule(trimmed) || is_markdown_image(trimmed) {
            continue;
        }

        let non_ws_chars: Vec<char> = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        if non_ws_chars.is_empty() {
            continue;
        }

        let non_ascii_count = non_ws_chars.iter().filter(|c| !c.is_ascii()).count();
        let ratio = non_ascii_count as f64 / non_ws_chars.len() as f64;
        if ratio > 0.7 && !is_legitimate_multilingual(&non_ws_chars) {
            issues.push(NoiseIssue {
                kind: NoiseKind::GarbledText,
                line: i + 1,
                context: truncate_context(line, 80),
                severity: Severity::Warning,
            });
            continue;
        }

        let mut consecutive_punct = 0;
        let mut has_punct_run = false;
        for ch in trimmed.chars() {
            if ch.is_ascii_punctuation() && !MARKDOWN_STRUCTURAL_PUNCT.contains(&ch) {
                consecutive_punct += 1;
                if consecutive_punct >= 4 {
                    has_punct_run = true;
                    break;
                }
            } else {
                consecutive_punct = 0;
            }
        }
        if has_punct_run {
            issues.push(NoiseIssue {
                kind: NoiseKind::GarbledText,
                line: i + 1,
                context: truncate_context(line, 80),
                severity: Severity::Warning,
            });
        }
    }

    issues
}

/// Detects empty headings (e.g., `# ` with no content).
fn detect_empty_headings(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if (1..=6).contains(&hash_count) {
                let rest = &trimmed[hash_count..];
                if rest.trim().is_empty() {
                    issues.push(NoiseIssue {
                        kind: NoiseKind::EmptyHeading,
                        line: i + 1,
                        context: truncate_context(line, 80),
                        severity: Severity::Error,
                    });
                }
            }
        }
    }

    issues
}

/// Counts unescaped pipe characters in a table row.
///
/// Escaped pipes (`\|`) are literal pipe characters inside cell content and
/// should NOT be counted as column delimiters.
fn count_unescaped_pipes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'|' && (i == 0 || bytes[i - 1] != b'\\') {
            count += 1;
        }
    }
    count
}

/// Detects broken pipe tables with inconsistent column counts.
///
/// Uses [`count_unescaped_pipes`] to ignore escaped pipes (`\|`) that appear
/// as literal content inside table cells.
fn detect_broken_tables(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();

    let mut table_start: Option<usize> = None;
    let mut header_col_count: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            table_start = None;
            header_col_count = None;
            continue;
        }

        let trimmed = line.trim();
        if trimmed.starts_with('|') {
            let col_count = count_unescaped_pipes(trimmed);
            if table_start.is_none() {
                table_start = Some(i);
                header_col_count = Some(col_count);
            } else if let Some(expected) = header_col_count {
                let is_separator = trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ');
                if !is_separator && col_count != expected {
                    issues.push(NoiseIssue {
                        kind: NoiseKind::BrokenTable,
                        line: i + 1,
                        context: truncate_context(line, 80),
                        severity: Severity::Warning,
                    });
                }
            }
        } else {
            table_start = None;
            header_col_count = None;
        }
    }

    issues
}

/// Detects orphaned list markers with no content.
fn detect_orphaned_list_markers(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();

        let is_orphaned_unordered = (trimmed == "-" || trimmed == "*" || trimmed == "+")
            || (trimmed.len() >= 2
                && (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ "))
                && trimmed[2..].trim().is_empty());

        let is_orphaned_ordered = if let Some(dot_pos) = trimmed.find('.') {
            let before_dot = &trimmed[..dot_pos];
            let after_dot = &trimmed[dot_pos + 1..];
            !before_dot.is_empty() && before_dot.chars().all(|c| c.is_ascii_digit()) && after_dot.trim().is_empty()
        } else {
            false
        };

        if is_orphaned_unordered || is_orphaned_ordered {
            issues.push(NoiseIssue {
                kind: NoiseKind::OrphanedListMarker,
                line: i + 1,
                context: truncate_context(line, 80),
                severity: Severity::Warning,
            });
        }
    }

    issues
}

/// Detects standalone small numbers that look like page number artifacts.
///
/// Only flags when at least 5 sequential or near-sequential standalone numbers
/// exist AND the sequential numbers span at least 20 lines apart (page-like
/// spacing). Clustered numbers (like table cells) are not flagged.
fn detect_page_number_artifacts(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut candidates: Vec<(usize, u32)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();
        if let Ok(num) = trimmed.parse::<u32>()
            && (1..=9999).contains(&num)
            && trimmed.len() <= 4
        {
            candidates.push((i, num));
        }
    }

    if candidates.len() < 5 {
        return Vec::new();
    }

    let values: Vec<u32> = candidates.iter().map(|(_, v)| *v).collect();
    let line_indices: Vec<usize> = candidates.iter().map(|(i, _)| *i).collect();
    let mut sequential_count = 0;
    let mut sequential_min_line = usize::MAX;
    let mut sequential_max_line = 0usize;
    for (idx, window) in values.windows(2).enumerate() {
        let diff = window[1].saturating_sub(window[0]);
        if (1..=3).contains(&diff) {
            sequential_count += 1;
            sequential_min_line = sequential_min_line.min(line_indices[idx]);
            sequential_max_line = sequential_max_line.max(line_indices[idx + 1]);
        }
    }

    if sequential_count < 4 {
        return Vec::new();
    }

    let span = sequential_max_line.saturating_sub(sequential_min_line);
    if span < 20 {
        return Vec::new();
    }

    candidates
        .iter()
        .map(|(i, _)| NoiseIssue {
            kind: NoiseKind::PageNumberArtifact,
            line: i + 1,
            context: truncate_context(lines[*i], 80),
            severity: Severity::Info,
        })
        .collect()
}

/// Returns true if the line is a pipe table row (starts with `|`).
fn is_pipe_table_row(trimmed: &str) -> bool {
    trimmed.starts_with('|')
}

/// Returns true if the line is a markdown image reference like `![alt](url)` or `![]()`.
/// Matches lines that consist entirely of a markdown image pattern (possibly with
/// surrounding whitespace already trimmed).
fn is_image_placeholder(trimmed: &str) -> bool {
    trimmed.starts_with("![")
}

/// Returns true if the line is a markdown image reference `![...](...)`
/// or an escaped variant `\[...\](...)`. Used to skip garbled-text detection
/// on lines that are purely image/link markup.
fn is_markdown_image(trimmed: &str) -> bool {
    if trimmed.starts_with("![") {
        return true;
    }
    if trimmed.starts_with("\\[") || trimmed.starts_with("\\!") {
        return true;
    }
    false
}

/// Returns true if the line looks like an RST grid table border.
///
/// RST grid table borders start with `+` and contain only `+`, `-`, `=`, `|`,
/// and spaces. Examples: `+---+---+---+`, `+===+===+`.
fn is_rst_grid_table_border(trimmed: &str) -> bool {
    trimmed.starts_with('+') && trimmed.chars().all(|c| matches!(c, '+' | '-' | '=' | '|' | ' '))
}

/// Detects lines that repeat 10+ times in the document (header/footer repetition).
///
/// Skips pipe table rows, image placeholders, table separator rows, and lines
/// with fewer than 20 non-whitespace characters (too short to be a meaningful
/// header/footer candidate).
///
/// To reduce false positives from legitimate repetitive content (e.g., ISO
/// standard column headers, Wikipedia navbox rows), candidates must also pass
/// a **periodicity check**: their occurrences must be roughly evenly spaced
/// (std_dev / mean_gap <= 0.5). Real page headers/footers appear at regular
/// intervals corresponding to page breaks, while content repetition is
/// irregular.
///
/// Lines that look like table column headers (all words Title Case or
/// UPPERCASE, under 40 chars) are also excluded.
///
/// Results are capped at 30 issues per document to avoid inflating noise
/// counts.
fn detect_header_footer_repetition(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    const MIN_OCCURRENCES: usize = 10;
    const MIN_NON_WS_CHARS: usize = 20;
    const MAX_ISSUES: usize = 30;
    const MAX_PERIODICITY_RATIO: f64 = 0.5;
    const TABLE_HEADER_MAX_LEN: usize = 40;

    let mut line_counts: HashMap<&str, Vec<usize>> = HashMap::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_pipe_table_row(trimmed) {
            continue;
        }

        if is_image_placeholder(trimmed) {
            continue;
        }

        if is_rst_grid_table_border(trimmed) {
            continue;
        }

        let non_ws_count = trimmed.chars().filter(|c| !c.is_whitespace()).count();
        if non_ws_count < MIN_NON_WS_CHARS {
            continue;
        }

        if trimmed.len() <= TABLE_HEADER_MAX_LEN && looks_like_table_header(trimmed) {
            continue;
        }

        line_counts.entry(trimmed).or_default().push(i);
    }

    let mut issues = Vec::new();
    for (content, positions) in &line_counts {
        if positions.len() >= MIN_OCCURRENCES && is_periodic(positions, MAX_PERIODICITY_RATIO) {
            for &pos in positions {
                issues.push(NoiseIssue {
                    kind: NoiseKind::HeaderFooterRepetition,
                    line: pos + 1,
                    context: truncate_context(content, 80),
                    severity: Severity::Warning,
                });
            }
        }
    }

    issues.sort_by_key(|issue| issue.line);

    issues.truncate(MAX_ISSUES);
    issues
}

/// Returns true if `positions` (sorted line indices) are roughly evenly spaced.
///
/// Computes the coefficient of variation (std_dev / mean) of the gaps between
/// consecutive positions. A ratio <= `max_ratio` indicates periodic repetition
/// (like page headers). A higher ratio means the repetition is irregular (like
/// repeated table content).
///
/// Returns `true` (periodic) when there are fewer than 3 positions, since we
/// cannot meaningfully assess periodicity.
fn is_periodic(positions: &[usize], max_ratio: f64) -> bool {
    if positions.len() < 3 {
        return true;
    }

    let gaps: Vec<f64> = positions.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let n = gaps.len() as f64;
    let mean = gaps.iter().sum::<f64>() / n;

    if mean < 1.0 {
        return false;
    }

    let variance = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    let cv = std_dev / mean;

    cv <= max_ratio
}

/// Returns true if the line looks like a table column header.
///
/// A table header line has ALL words either Title Case (first char uppercase,
/// rest lowercase) or fully UPPERCASE. This catches patterns like
/// "Item Content", "Remark", "Prerequisite", "TEST CASE ID".
fn looks_like_table_header(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }

    words.iter().all(|word| {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => {
                if !first.is_alphabetic() {
                    return false;
                }
                let rest: String = chars.collect();
                let is_title_case =
                    first.is_uppercase() && rest.chars().all(|c| !c.is_alphabetic() || c.is_lowercase());
                let is_upper = first.is_uppercase() && rest.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
                is_title_case || is_upper
            }
            None => true,
        }
    })
}

/// Detects footnote references `[^N]` without corresponding `[^N]:` definitions.
fn detect_dangling_references(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut references: Vec<(usize, String)> = Vec::new();
    let mut definitions: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }

        let mut start = 0;
        while let Some(pos) = line[start..].find("[^") {
            let abs_pos = start + pos;
            let after = &line[abs_pos + 2..];
            if let Some(close) = after.find(']') {
                let label = after[..close].to_string();
                let after_close = &after[close + 1..];
                if after_close.starts_with(':') {
                    definitions.insert(label);
                } else if !label.is_empty() && !after_close.starts_with('(') {
                    references.push((i, label));
                }
                start = abs_pos + 2 + close + 1;
            } else {
                break;
            }
        }
    }

    references
        .into_iter()
        .filter(|(_, label)| !definitions.contains(label))
        .map(|(i, _)| NoiseIssue {
            kind: NoiseKind::DanglingReference,
            line: i + 1,
            context: truncate_context(lines[i], 80),
            severity: Severity::Warning,
        })
        .collect()
}

/// Detects excessive heading density (more than 2x headings vs paragraphs when heading count > 10).
fn detect_excessive_heading_density(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut heading_count = 0usize;
    let mut paragraph_count = 0usize;

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if (1..=6).contains(&hash_count) {
                heading_count += 1;
                continue;
            }
        }

        if trimmed.starts_with('|')
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || (trimmed.len() >= 2 && trimmed.as_bytes()[0].is_ascii_digit() && trimmed.contains(". "))
        {
            continue;
        }

        paragraph_count += 1;
    }

    if heading_count > 2 * paragraph_count && heading_count > 10 {
        vec![NoiseIssue {
            kind: NoiseKind::ExcessiveHeadingDensity,
            line: 1,
            context: format!("{heading_count} headings vs {paragraph_count} paragraphs"),
            severity: Severity::Warning,
        }]
    } else {
        Vec::new()
    }
}

/// Longest span (in bytes, between `&` and `;`) a `&...;` run may have and still plausibly be
/// an HTML character reference — both numeric (`&#x0000;`) and the longest named entities fit
/// well under this. ~keep
const MAX_ENTITY_REFERENCE_LEN: usize = 10;

/// Detects unresolved HTML entities like `&#10;`, `&#x0A;`, `&amp;`, `&nbsp;` outside code blocks.
///
/// These are extraction artifacts where the HTML-to-markdown conversion failed to
/// decode character references.
fn detect_unresolved_html_entities(lines: &[&str], code_ranges: &[CodeRange]) -> Vec<NoiseIssue> {
    let mut issues = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if in_code_block(i, code_ranges) {
            continue;
        }
        if line_has_unresolved_entity(line) {
            issues.push(NoiseIssue {
                kind: NoiseKind::UnresolvedHtmlEntity,
                line: i + 1,
                context: truncate_context(line, 80),
                severity: Severity::Warning,
            });
        }
    }

    issues
}

/// Whether `line` contains at least one unresolved HTML character reference. Scans byte
/// positions for `&`, and for each candidate `&...;` span checks the reference shape.
fn line_has_unresolved_entity(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] != b'&' {
            pos += 1;
            continue;
        }
        let rest = &line[pos..];
        let Some(semi) = rest.find(';') else {
            pos += 1;
            continue;
        };
        if semi <= MAX_ENTITY_REFERENCE_LEN && is_entity_reference(&rest[1..semi]) {
            return true;
        }
        pos += semi + 1;
    }
    false
}

/// Whether `entity` (the text between `&` and `;`) is shaped like a numeric (`#10`, `#x0A`) or
/// named (`amp`, `nbsp`) HTML character reference.
fn is_entity_reference(entity: &str) -> bool {
    let is_numeric = entity.starts_with('#')
        && entity.len() > 1
        && entity[1..].chars().all(|c| c.is_ascii_digit() || c == 'x' || c == 'X');
    let is_named = !entity.is_empty() && entity.chars().all(|c| c.is_ascii_alphanumeric());
    is_numeric || is_named
}

/// Runs all noise detection heuristics and produces a diagnostic report.
pub fn detect_noise(markdown: &str) -> DiagnosticReport {
    let lines: Vec<&str> = markdown.lines().collect();
    let code_ranges = find_code_ranges(&lines);

    let mut issues = Vec::new();
    issues.extend(detect_html_remnants(&lines, &code_ranges));
    issues.extend(detect_excessive_whitespace(&lines, &code_ranges));
    issues.extend(detect_garbled_text(&lines, &code_ranges));
    issues.extend(detect_empty_headings(&lines, &code_ranges));
    issues.extend(detect_broken_tables(&lines, &code_ranges));
    issues.extend(detect_orphaned_list_markers(&lines, &code_ranges));
    issues.extend(detect_page_number_artifacts(&lines, &code_ranges));
    issues.extend(detect_header_footer_repetition(&lines, &code_ranges));
    issues.extend(detect_dangling_references(&lines, &code_ranges));
    issues.extend(detect_excessive_heading_density(&lines, &code_ranges));
    issues.extend(detect_unresolved_html_entities(&lines, &code_ranges));

    let total_lines = lines.len();
    let summary = build_summary(&issues, total_lines);

    DiagnosticReport { issues, summary }
}

/// Builds an aggregated summary from a list of issues.
fn build_summary(issues: &[NoiseIssue], total_lines: usize) -> NoiseSummary {
    let mut by_kind: HashMap<String, usize> = HashMap::new();
    let mut by_severity: HashMap<String, usize> = HashMap::new();

    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    let mut info_count = 0usize;

    for issue in issues {
        *by_kind.entry(issue.kind.as_str().to_string()).or_insert(0) += 1;
        *by_severity.entry(issue.severity.as_str().to_string()).or_insert(0) += 1;

        match issue.severity {
            Severity::Error => error_count += 1,
            Severity::Warning => warning_count += 1,
            Severity::Info => info_count += 1,
        }
    }

    let weighted = error_count as f64 * 0.3 + warning_count as f64 * 0.1 + info_count as f64 * 0.02;
    let denominator = (total_lines / 50).max(1) as f64;
    let noise_score = (weighted / denominator).min(1.0);

    NoiseSummary {
        total_issues: issues.len(),
        by_kind,
        by_severity,
        noise_score,
    }
}

#[cfg(test)]
mod tests;
