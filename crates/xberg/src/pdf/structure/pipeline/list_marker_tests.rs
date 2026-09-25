use super::{is_bare_detached_list_marker, is_bare_list_marker, looks_like_list_item};

#[test]
fn bare_markers_are_detected() {
    assert!(is_bare_list_marker("1."));
    assert!(is_bare_list_marker("12)"));
    assert!(is_bare_list_marker("a."));
    assert!(is_bare_list_marker("a)"));
    assert!(is_bare_list_marker("I."));
    assert!(is_bare_list_marker("(1)"));
    assert!(is_bare_list_marker("(2)"));
    assert!(is_bare_list_marker("[1]"));
    assert!(is_bare_list_marker("•"));
}

#[test]
fn prose_fragments_are_not_bare_markers() {
    assert!(!is_bare_list_marker("etc."));
    assert!(!is_bare_list_marker("Inc."));
    assert!(!is_bare_list_marker("(appendix)"));
    assert!(!is_bare_list_marker("Item"));
    assert!(!is_bare_list_marker(""));
}

/// The general [`is_bare_list_marker`] still accepts a lone `*` and a
/// bracketed integer -- only the narrower detached-reattachment predicate
/// rejects them. See `EXCLUDE_AMBIGUOUS_DETACHED_MARKERS`.
#[test]
fn detached_predicate_rejects_the_ambiguous_shapes_the_general_one_still_accepts() {
    assert!(
        is_bare_list_marker("*"),
        "general predicate must still accept a lone '*'"
    );
    assert!(
        is_bare_list_marker("[42]"),
        "general predicate must still accept a bracketed integer"
    );
    assert!(
        !is_bare_detached_list_marker("*"),
        "a lone '*' is also a multiplication sign; the detached pass must reject it"
    );
    assert!(
        !is_bare_detached_list_marker("[42]"),
        "a bracketed integer is a printed paragraph number; the detached pass must reject it"
    );
}

/// Every shape the task's evidence names as "good, keep" must survive the
/// tightening on the detached-reattachment predicate.
#[test]
fn detached_predicate_still_accepts_the_unambiguous_shapes() {
    assert!(is_bare_detached_list_marker("-"));
    assert!(is_bare_detached_list_marker("–"));
    assert!(is_bare_detached_list_marker("—"));
    assert!(is_bare_detached_list_marker("(1)"));
    assert!(is_bare_detached_list_marker("(k)"));
    assert!(is_bare_detached_list_marker("1."));
    assert!(is_bare_detached_list_marker("f."));
}

#[test]
fn newline_separated_marker_and_text_is_a_list_item() {
    assert!(looks_like_list_item("1.\nÉnumération 1"));
    assert!(looks_like_list_item("1. First point"));
    assert!(looks_like_list_item("123. One hundred twenty-third point"));
    assert!(looks_like_list_item("999. Nine hundred ninety-ninth point"));
    assert!(!looks_like_list_item("1000. Four-digit identifier"));
    assert!(looks_like_list_item("viii. eighth item"));
    assert!(looks_like_list_item("(2)\nsecond item"));
    assert!(looks_like_list_item("[1] bracketed item"));
}

#[test]
fn four_digit_year_is_not_a_list_item() {
    assert!(!looks_like_list_item("2023. A total of 3 trucks were used"));
}

#[test]
fn section_headings_are_not_list_items() {
    assert!(!looks_like_list_item("3.2 Methods"));
    assert!(!looks_like_list_item("IV. Results"));
    assert!(!looks_like_list_item("1. INTRODUCTION"));
}

#[test]
fn prose_words_ending_with_period_are_not_list_markers() {
    assert!(!looks_like_list_item("tua. At vero eos et accusam"));
    assert!(!looks_like_list_item("etc. and more prose"));
    assert!(looks_like_list_item("a. first item"));
    assert!(looks_like_list_item("iv. fourth item"));
}

#[test]
fn typographic_dash_requires_an_inline_body() {
    assert!(looks_like_list_item("– first item"));
    assert!(looks_like_list_item("—\tsecond item"));
    assert!(looks_like_list_item("– “quoted item”"));
    assert!(looks_like_list_item("— (parenthesized item)"));
    assert!(!looks_like_list_item("–\n457"));
    assert!(!looks_like_list_item("– \n457"));
    assert!(!looks_like_list_item("—\t\nbody"));
    assert!(!looks_like_list_item("–\n8 show the remaining figures"));
    assert!(!looks_like_list_item("—continuation"));
}

/// #### FAILS against unfixed code
/// Both assertions currently evaluate to `true` (unfixed
/// `looks_like_list_item` accepts any `(N) <alphabetic>` line), so
/// `assert!(!looks_like_list_item(...))` panics with `assertion failed:
/// !looks_like_list_item("(2) additional on-street parallel parking
/// spaces")` (and the `(7)` sibling) on unfixed code.
#[test]
fn parenthesized_quantity_clarifications_are_not_list_items() {
    assert!(!looks_like_list_item(
        "(2) additional on-street parallel parking spaces"
    ));
    assert!(!looks_like_list_item("(7) on-street spaces on Lake Pointe Parkway"));
    assert!(!looks_like_list_item("(3) additional off-street spaces"));
    assert!(!looks_like_list_item("(9) exceptions apply"));
}

/// Lettered sub-items in parentheses are genuine markers in this same
/// ordinance and must survive the quantity-clarification heuristic above
/// (it is scoped to *numeric* parenthesized markers only).
#[test]
fn parenthesized_letter_markers_remain_list_items() {
    assert!(looks_like_list_item("(a) Front setback: 25'"));
    assert!(looks_like_list_item("(b) Side setback: 0'/6'"));
    assert!(looks_like_list_item("(c) Street side setback: Lot 1 - 15'"));
}

/// A capitalized, space-separated numeric parenthesized marker is a
/// genuine enumerated item (a new sentence), not a quantity
/// clarification, and must still be accepted.
#[test]
fn capitalized_parenthesized_numeric_markers_remain_list_items() {
    assert!(looks_like_list_item("(1) First point"));
    assert!(looks_like_list_item("(2) Second point"));
}

#[test]
fn author_initials_are_not_list_markers() {
    assert!(!looks_like_list_item(
        "O. Sanni, A.P.I. Popoola / Data in Brief 22 (2019) 451"
    ));
    assert!(!looks_like_list_item("O. Sanni, A. Popoola / Data in Brief"));
    assert!(looks_like_list_item("A. First item"));
    assert!(looks_like_list_item("a. first item"));
    assert!(looks_like_list_item("A. Compare input, output / behavior"));
}

#[test]
fn arrow_bullets_are_recognized_only_at_the_start() {
    assert!(is_bare_list_marker("➢"));
    assert!(is_bare_detached_list_marker("➢"));
    assert!(looks_like_list_item("➢ First item"));
    assert!(!looks_like_list_item("Follow A ➢ B"));
    assert!(!is_bare_list_marker("➢ First item"));
}
