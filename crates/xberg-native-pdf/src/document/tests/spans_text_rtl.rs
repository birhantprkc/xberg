use super::super::*;
use super::common::*;

#[test]
fn test_normalize_kangxi_no_radicals() {
    let text = "Hello World";
    let result = PdfDocument::normalize_kangxi_radicals(text);
    assert_eq!(result, text);
}

#[test]
fn test_normalize_kangxi_with_radicals() {
    // U+2F00 is Kangxi Radical One ~keep
    let text = "\u{2F00}";
    let result = PdfDocument::normalize_kangxi_radicals(text);
    // Should be normalized to a CJK unified ideograph ~keep
    assert_ne!(result, text);
}

#[test]
fn test_normalize_arabic_no_presentation_forms() {
    let text = "Hello World";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    assert_eq!(result, text);
}

#[test]
fn test_normalize_arabic_alef_presentation_form() {
    // U+FE8D is Arabic Alef isolated form ~keep
    let text = "\u{FE8D}";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    // Should be normalized to base Alef (U+0627) ~keep
    assert!(result.contains('\u{0627}'));
}

#[test]
fn test_normalize_arabic_lam_alef_ligature() {
    // U+FEFB is Lam-Alef ligature ~keep
    let text = "\u{FEFB}";
    let result = PdfDocument::normalize_arabic_presentation_forms(text);
    // Should become Lam (U+0644) ~keep
    assert!(result.contains('\u{0644}'));
}

#[test]
fn test_reverse_rtl_preshaped_single_span() {
    // "ArabicCIDTrueType.pdf" shape: one span per line, glyphs in
    // visual / right-to-left rendering order, mixing presentation
    // form `ﳋ` (U+FCCB) with base Arabic characters. The helper
    // must reverse this into reading order so downstream consumers
    // see logical Arabic even though the content stream is visual. ~keep
    let mut spans = vec![
        make_rtl_test_span(
            "\u{0629}\u{064A}\u{0628}\u{0631}\u{0639}\u{0644}\u{0627} \
                                \u{0637}\u{0648}\u{0637}\u{FCCB}\u{0627} \
                                \u{0639}\u{0627}\u{0648}\u{0646}\u{0627}",
            100.0,
            700.0,
        ),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    // After reversal, the first span should read as
    // "انواع اﳋطوط العربية" — the logical reading order. The
    // exact string comparison is the reversal of the input. ~keep
    assert_eq!(
        spans[0].text,
        "\u{0627}\u{0646}\u{0648}\u{0627}\u{0639} \
             \u{0627}\u{FCCB}\u{0637}\u{0648}\u{0637} \
             \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}",
        "Pre-shaped Arabic single span must be reversed into reading order"
    );
    // Other non-RTL spans must be untouched. ~keep
    assert_eq!(spans[1].text, "other content");
    assert_eq!(spans[2].text, "more content");
    assert_eq!(spans[3].text, "tail");
}

#[test]
fn test_reverse_rtl_logical_order_base_arabic_untouched() {
    // Most Arabic PDFs store text in logical (reading) order using
    // base characters (U+0621-U+06FF) and rely on the renderer to
    // apply shaping at display time. xberg-native-pdf must leave those
    // spans alone — reversing them would garble correct output.
    //
    // The string below is "انواع الخطوط العربية" entirely composed
    // of base Arabic code points (no presentation forms). Gate:
    // `has_presentation_form` stays false, no reversal happens. ~keep
    let logical = "\u{0627}\u{0646}\u{0648}\u{0627}\u{0639} \
                       \u{0627}\u{0644}\u{062E}\u{0637}\u{0648}\u{0637} \
                       \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}";
    let mut spans = vec![
        make_rtl_test_span(logical, 100.0, 700.0),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    assert_eq!(
        spans[0].text, logical,
        "Logical-order base-Arabic span must NOT be reversed"
    );
}

#[test]
fn test_reverse_rtl_short_rtl_span_not_touched_by_pass0() {
    // Pass 0 requires at least 4 non-whitespace characters. A
    // two-character Arabic snippet must not trigger reversal even
    // though it contains presentation forms. ~keep
    let mut spans = vec![
        make_rtl_test_span("\u{FB7F}\u{FEB3}", 100.0, 700.0),
        make_rtl_test_span("other content", 100.0, 680.0),
        make_rtl_test_span("more content", 100.0, 660.0),
        make_rtl_test_span("tail", 100.0, 640.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    assert_eq!(spans[0].text, "\u{FB7F}\u{FEB3}");
}

#[test]
fn test_reverse_rtl_pass0_leaves_ltr_alone() {
    // Pure Latin spans never trip the RTL heuristic — `rtl_count`
    // is zero so the majority gate fails. ~keep
    let mut spans = vec![
        make_rtl_test_span("The quick brown fox jumps over", 100.0, 700.0),
        make_rtl_test_span("the lazy dog repeatedly.", 100.0, 680.0),
        make_rtl_test_span("Latin content here.", 100.0, 660.0),
        make_rtl_test_span("Final line.", 100.0, 640.0),
    ];
    let before: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    let after: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();
    assert_eq!(before, after, "Pure-Latin spans must not be reversed by the RTL pass");
}

// The common CID-TrueType shape — one span PER WORD, each word's
// characters already in LOGICAL order (Presentation Forms), laid out
// right-to-left so the row-aware sort hands them to us left-to-right
// (x ascending: last logical word first). The pass must (A) NOT
// char-reverse the per-word spans — they're already logical — and
// (B) reverse the WORD order so they read right-to-left. Phrase:
// "اﻧﻮاع اﳋﻄﻮط اﻟﻌﺮﺑﻴﺔ" ("types of Arabic fonts"). ~keep
#[test]
fn test_reverse_rtl_per_word_logical_spans_reorder_not_charflip() {
    // Spans in x-ascending order (as emitted by the row-aware sort):
    // العربية (leftmost) … انواع (rightmost / logically first). ~keep
    let mut spans = vec![
        make_rtl_test_span("اﻟﻌﺮﺑﻴﺔ", 160.0, 700.0),
        make_rtl_test_span(" ", 277.0, 700.0),
        make_rtl_test_span("اﳋﻄﻮط", 288.0, 700.0),
        make_rtl_test_span(" ", 409.0, 700.0),
        make_rtl_test_span("اﻧﻮاع", 420.0, 700.0),
    ];
    PdfDocument::reverse_rtl_visual_order_runs(&mut spans);
    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    // (B) word order reversed to logical right-to-left: ~keep
    assert_eq!(
        texts,
        vec!["اﻧﻮاع", " ", "اﳋﻄﻮط", " ", "اﻟﻌﺮﺑﻴﺔ"],
        "per-word RTL spans must be reordered into logical word order \
             without char-flipping (got {texts:?})"
    );
}

// Grapheme-aware RTL reversal keeps Arabic combining marks bound to
// their base letter (vs. a naive chars().rev() that floats them off). ~keep
#[test]
fn test_reverse_rtl_keeping_marks_keeps_diacritics_attached() {
    // قِطّ = QAF + KASRA(U+0650) + TAH + SHADDA(U+0651). Reversing must
    // keep each mark immediately after its base, not lead the string. ~keep
    let src = "\u{0642}\u{0650}\u{0637}\u{0651}";
    let out = PdfDocument::reverse_rtl_keeping_marks(src);
    // Expected: base order reversed (TAH+SHADDA group, then QAF+KASRA group). ~keep
    assert_eq!(out, "\u{0637}\u{0651}\u{0642}\u{0650}");
    // No combining mark ever leads a base it doesn't belong to: every
    // diacritic is immediately preceded by a non-diacritic. ~keep
    let chars: Vec<char> = out.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if crate::text::rtl_detector::is_rtl_diacritic(*c as u32) {
            assert!(
                i > 0 && !crate::text::rtl_detector::is_rtl_diacritic(chars[i - 1] as u32),
                "diacritic at {i} is detached from its base"
            );
        }
    }
}

/// A neutral-only span ("<space><comma>") inside a pure-RTL run carries its
/// glyphs in visual draw order; emitting it under an RTL run must reverse it
/// to logical order ("<comma><space>") so the comma re-attaches to the word
/// it follows. Reproduces the wiki-cat-he `הטורפים, ממשפחת` case.
#[test]
fn test_push_span_text_bidi_reverses_neutral_span_in_rtl_run() {
    let span = make_rtl_test_span(" ,", 270.0, 700.0);
    let mut out = String::from("\u{05D4}\u{05D8}\u{05D5}\u{05E8}");
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert!(out.ends_with(", "), "neutral span not reversed to logical: {out:?}");
    assert!(!out.ends_with(" ,"), "visual order leaked into output: {out:?}");
}

/// The same neutral-only span in a non-RTL run (rtl_run = false) is emitted
/// verbatim — LTR text keeps visual == logical order, so reversal would be
/// wrong. Pins the no-regression contract for LTR documents.
#[test]
fn test_push_span_text_bidi_keeps_neutral_span_in_ltr_run() {
    let span = make_rtl_test_span(" ,", 270.0, 700.0);
    let mut out = String::from("word");
    PdfDocument::push_span_text_bidi(&mut out, &span, false);
    assert_eq!(out, "word ,", "LTR neutral span must be emitted verbatim");
}

/// A neutral+single-number span inside a pure-RTL run is reversed to logical
/// order with the DIGIT RUN KEPT FORWARD (UAX #9 L2): visual `2009,` →
/// logical `,2009` (the comma re-attaches to the preceding word; `2009` is
/// never flipped to `9002`). Guards `is_reversible_rtl_numeric_span`.
#[test]
fn test_push_span_text_bidi_reverses_neutral_number_keeping_digits() {
    let span = make_rtl_test_span("2009,", 270.0, 700.0);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert_eq!(out, ",2009", "neutral+number span: reverse order, keep 2009 forward");
}

/// A span with TWO digit runs joined by a hyphen (a year range / ORCID) must
/// NOT be reversed — only a single maximal digit run qualifies.
#[test]
fn test_push_span_text_bidi_does_not_reverse_multi_number_span() {
    let span = make_rtl_test_span("2009-2010", 270.0, 700.0);
    let mut out = String::new();
    PdfDocument::push_span_text_bidi(&mut out, &span, true);
    assert_eq!(out, "2009-2010", "multi-digit-run span must be emitted verbatim");
}

#[test]
fn test_is_reversible_rtl_neutral_span_classification() {
    // Reversible: at least one reorderable punctuation mark + ≥2 chars. ~keep
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" ,"));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" ."));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(". "));
    assert!(PdfDocument::is_reversible_rtl_neutral_span(" \u{060C}")); // Arabic comma ~keep
    // Not reversible: single char (reverses to itself), bare spaces, or
    // anything carrying a letter / digit / bracket / quote. ~keep
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(","));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span("  "));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" 9"));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" )"));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span(" \""));
    assert!(!PdfDocument::is_reversible_rtl_neutral_span("a,"));
}

#[test]
fn test_strip_interior_arabic_spaces() {
    // Spurious space between two Arabic letters (cursive join) is dropped.
    // قِ ل ا  →  قِلا  (space between kasra-marked qaf and lam removed). ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642}\u{0650} \u{0644}\u{0627}"),
        "\u{0642}\u{0650}\u{0644}\u{0627}"
    );
    // A combining mark adjacent to the space does not hide the base letter. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642} \u{0650}\u{0644}"),
        "\u{0642}\u{0650}\u{0644}"
    );
    // Leading / trailing spaces (real word-break candidates) are preserved. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces(" \u{0642}\u{0644} "),
        " \u{0642}\u{0644} "
    );
    // Non-Arabic flanks are left alone: Hebrew (non-cursive) keeps its space. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{05E9} \u{05DC}"),
        "\u{05E9} \u{05DC}"
    );
    // Space between an Arabic letter and a digit is a real boundary — kept. ~keep
    assert_eq!(PdfDocument::strip_interior_arabic_spaces("\u{0642} 5"), "\u{0642} 5");
    // No spaces → fast path returns the input unchanged. ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0642}\u{0644}"),
        "\u{0642}\u{0644}"
    );
    // Joining-type discriminator: a space AFTER a right-joining-only letter
    // (reh ر) is kept — the join already breaks there, so it may be a real
    // word boundary and stripping it would concatenate two words.
    // بحر ما  →  unchanged (reh before the space). ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0628}\u{062D}\u{0631} \u{0645}\u{0627}"),
        "\u{0628}\u{062D}\u{0631} \u{0645}\u{0627}"
    );
    // A space after a DUAL-joining letter (beh ب) unambiguously broke a
    // cursive join → still stripped.  كتب لا  →  كتبلا ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0643}\u{062A}\u{0628} \u{0644}\u{0627}"),
        "\u{0643}\u{062A}\u{0628}\u{0644}\u{0627}"
    );
    // SHATTER: a producer that exploded one word into glyphs (a space between
    // most letter pairs) has every interior space stripped.  ة لي ص ف → ةليصف ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces("\u{0629} \u{0644}\u{064A} \u{0635} \u{0641}"),
        "\u{0629}\u{0644}\u{064A}\u{0635}\u{0641}"
    );
    // NOT a shatter: sparse spaces in genuine multi-word text are kept (the
    // density stays below half the inter-letter gaps).  دار سلام بلد  unchanged ~keep
    assert_eq!(
        PdfDocument::strip_interior_arabic_spaces(
            "\u{062F}\u{0627}\u{0631} \u{0633}\u{0644}\u{0627}\u{0645} \u{0628}\u{0644}\u{062F}"
        ),
        "\u{062F}\u{0627}\u{0631} \u{0633}\u{0644}\u{0627}\u{0645} \u{0628}\u{0644}\u{062F}"
    );
}

#[test]
fn test_mcid_run_is_pure_rtl() {
    let pure_rtl = vec![
        make_rtl_test_span("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", 100.0, 700.0),
        make_rtl_test_span(" ,", 90.0, 700.0),
    ];
    assert!(PdfDocument::mcid_run_is_pure_rtl(&pure_rtl));
    // RTL + Latin → not pure-RTL (full UAX #9 deferred). ~keep
    let mixed = vec![
        make_rtl_test_span("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", 100.0, 700.0),
        make_rtl_test_span("World", 200.0, 700.0),
    ];
    assert!(!PdfDocument::mcid_run_is_pure_rtl(&mixed));
    // No RTL at all → not pure-RTL. ~keep
    let ltr = vec![make_rtl_test_span("Hello", 100.0, 700.0)];
    assert!(!PdfDocument::mcid_run_is_pure_rtl(&ltr));
}

#[test]
fn test_normalize_arabic_hamza() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE80}");
    assert!(result.contains('\u{0621}'));
}

#[test]
fn test_normalize_arabic_beh() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE8F}");
    assert!(result.contains('\u{0628}'));
}

#[test]
fn test_normalize_arabic_teh_marbuta() {
    let result = PdfDocument::normalize_arabic_presentation_forms("\u{FE93}");
    assert!(result.contains('\u{0629}'));
}

#[test]
fn test_normalize_arabic_dal_to_yeh_range() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEA9}").contains('\u{062F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAB}").contains('\u{0630}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAD}").contains('\u{0631}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEAF}").contains('\u{0632}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB1}").contains('\u{0633}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB5}").contains('\u{0634}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEB9}").contains('\u{0635}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEBD}").contains('\u{0636}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC1}").contains('\u{0637}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC5}").contains('\u{0638}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEC9}").contains('\u{0639}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FECD}").contains('\u{063A}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED1}").contains('\u{0641}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED5}").contains('\u{0642}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FED9}").contains('\u{0643}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEDD}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE1}").contains('\u{0645}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE5}").contains('\u{0646}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEE9}").contains('\u{0647}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEED}").contains('\u{0648}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEEF}").contains('\u{0649}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF1}").contains('\u{064A}'));
}

#[test]
fn test_normalize_arabic_diacritics() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE70}").contains('\u{064B}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE71}").contains('\u{064B}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE72}").contains('\u{064C}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE74}").contains('\u{064D}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE76}").contains('\u{064E}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE77}").contains('\u{064E}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE78}").contains('\u{064F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE79}").contains('\u{064F}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7A}").contains('\u{0650}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7B}").contains('\u{0650}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7C}").contains('\u{0651}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7D}").contains('\u{0651}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7E}").contains('\u{0652}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE7F}").contains('\u{0652}'));
}

#[test]
fn test_normalize_arabic_lam_alef_ligatures() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF5}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF7}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEF9}").contains('\u{0644}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FEFB}").contains('\u{0644}'));
}

#[test]
fn test_normalize_arabic_alef_variants() {
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE81}").contains('\u{0622}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE83}").contains('\u{0623}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE85}").contains('\u{0624}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE87}").contains('\u{0625}'));
    assert!(PdfDocument::normalize_arabic_presentation_forms("\u{FE89}").contains('\u{0626}'));
}

#[test]
fn test_normalize_arabic_mixed_text() {
    let result = PdfDocument::normalize_arabic_presentation_forms("Hello \u{FE8D} World");
    assert!(result.contains("Hello"));
    assert!(result.contains("World"));
    assert!(result.contains('\u{0627}'));
}
