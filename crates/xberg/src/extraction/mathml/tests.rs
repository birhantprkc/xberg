use super::*;

/// Helper: parse a MathML XML fragment and return rendered LaTeX.
fn mathml_to_latex(inner: &str) -> String {
    let xml = format!(r#"<math xmlns="http://www.w3.org/1998/Math/MathML">{}</math>"#, inner);
    let mut budget = SecurityBudget::with_defaults();
    convert_mathml_str_to_latex(&xml, &mut budget).expect("conversion ok")
}

/// OpenOffice writes its formula objects with a prefixed namespace and its
/// own DTD, which a real ODF document embeds verbatim.
/// OpenOffice writes its stretchy fences as private use codepoints, which
/// no renderer can display. One real document carried them into its LaTeX.
#[test]
fn test_private_use_characters_are_dropped() {
    let xml = "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mrow>\
<mi>F</mi><mo>\u{E09E}</mo><mn>1</mn><mo>\u{E09F}</mo>\
<mfenced open=\"\u{E09E}\" close=\"\u{E09F}\"><mn>2</mn></mfenced></mrow></math>";

    let mut budget = SecurityBudget::with_defaults();
    let latex = convert_mathml_str_to_latex(xml, &mut budget).expect("converts");

    assert!(
        !latex.chars().any(is_private_use),
        "no private use character survives: {latex:?}"
    );
    assert!(
        latex.contains('F') && latex.contains('1'),
        "the real content stays: {latex}"
    );
}

#[test]
fn test_prefixed_mathml_with_the_openoffice_doctype() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE math:math PUBLIC "-//OpenOffice.org//DTD Modified W3C MathML 1.01//EN" "math.dtd">
<math:math xmlns:math="http://www.w3.org/1998/Math/MathML">
 <math:semantics>
  <math:mrow>
   <math:mn>1</math:mn>
   <math:mo math:stretchy="false">+</math:mo>
   <math:mn>2</math:mn>
  </math:mrow>
 </math:semantics>
</math:math>"#;

    let mut budget = SecurityBudget::with_defaults();
    let latex = convert_mathml_str_to_latex(xml, &mut budget).expect("converts");
    assert!(
        !latex.trim().is_empty(),
        "a formula with a DOCTYPE must convert, got {latex:?}"
    );
    assert!(latex.contains('1') && latex.contains('2'), "operands survive: {latex}");
}

#[test]
fn test_mi_plain_text() {
    assert_eq!(mathml_to_latex("<mi>x</mi>"), "x");
}

#[test]
fn test_mn_number() {
    assert_eq!(mathml_to_latex("<mn>42</mn>"), "42");
}

#[test]
fn test_mo_unicode_operator() {
    assert_eq!(mathml_to_latex("<mo>\u{00D7}</mo>"), "\\times ");
}

#[test]
fn test_numeric_char_ref_with_trailing_comment_is_not_duplicated() {
    // Real-world MathML (e.g. EPUB accessibility test suites) commonly
    // annotates a numeric character reference with a same-content XML
    // comment: `<mo>&#x222B;<!-- ∫ --></mo>`. The comment must not be
    // rendered a second time alongside the decoded entity. ~keep
    assert_eq!(mathml_to_latex("<mo>&#x222B;<!-- \u{222B} --></mo>"), "\\int ");
    assert_eq!(
        mathml_to_latex("<mi mathvariant=\"normal\">&#x221E;<!-- \u{221E} --></mi>"),
        "\\infty "
    );
}

#[test]
fn test_mtext_wraps_in_text_command() {
    assert_eq!(mathml_to_latex("<mtext>hello world</mtext>"), "\\text{hello world}");
}

#[test]
fn test_ms_string_literal() {
    assert_eq!(mathml_to_latex("<ms>abc</ms>"), "abc");
}

#[test]
fn test_mrow_concatenates_children() {
    assert_eq!(mathml_to_latex("<mrow><mi>x</mi><mo>+</mo><mi>y</mi></mrow>"), "x+y");
}

#[test]
fn test_mfrac() {
    assert_eq!(mathml_to_latex("<mfrac><mn>1</mn><mn>2</mn></mfrac>"), "\\frac{1}{2}");
}

#[test]
fn test_msup() {
    assert_eq!(mathml_to_latex("<msup><mi>x</mi><mn>2</mn></msup>"), "x^{2}");
}

#[test]
fn test_msub() {
    assert_eq!(mathml_to_latex("<msub><mi>a</mi><mi>n</mi></msub>"), "a_{n}");
}

#[test]
fn test_msubsup() {
    assert_eq!(
        mathml_to_latex("<msubsup><mi>x</mi><mi>i</mi><mn>2</mn></msubsup>"),
        "x_{i}^{2}"
    );
}

#[test]
fn test_msqrt_no_degree() {
    assert_eq!(mathml_to_latex("<msqrt><mi>x</mi></msqrt>"), "\\sqrt{x}");
}

#[test]
fn test_mroot_with_degree() {
    assert_eq!(mathml_to_latex("<mroot><mi>x</mi><mn>3</mn></mroot>"), "\\sqrt[3]{x}");
}

#[test]
fn test_mfenced_default_parens() {
    assert_eq!(mathml_to_latex("<mfenced><mi>x</mi></mfenced>"), "\\left(x\\right)");
}

#[test]
fn test_mfenced_brackets_multiple_elements() {
    assert_eq!(
        mathml_to_latex(r#"<mfenced open="[" close="]"><mi>a</mi><mi>b</mi></mfenced>"#),
        "\\left[a,b\\right]"
    );
}

#[test]
fn test_munder() {
    assert_eq!(
        mathml_to_latex("<munder><mi>lim</mi><mi>n</mi></munder>"),
        "\\underset{n}{lim}"
    );
}

#[test]
fn test_mover_hat_accent() {
    // A bare `^` inside `\overset` is unparseable LaTeX ("expected group
    // after ^"); accent characters must map to accent macros.
    assert_eq!(mathml_to_latex("<mover><mi>x</mi><mo>^</mo></mover>"), "\\hat{x}");
}

#[test]
fn test_mover_accent_family() {
    assert_eq!(
        mathml_to_latex("<mover><mi>x</mi><mo>\u{02DC}</mo></mover>"),
        "\\tilde{x}"
    );
    assert_eq!(
        mathml_to_latex("<mover><mi>q</mi><mo>\u{02D9}</mo></mover>"),
        "\\dot{q}"
    );
    assert_eq!(
        mathml_to_latex("<mover><mi>y</mi><mo>\u{00AF}</mo></mover>"),
        "\\bar{y}"
    );
    assert_eq!(
        mathml_to_latex("<mover><mi>v</mi><mo>\u{2192}</mo></mover>"),
        "\\vec{v}"
    );
    // Multi-glyph base widens to the stretched forms.
    assert_eq!(
        mathml_to_latex("<mover><mrow><mi>a</mi><mi>b</mi></mrow><mo>\u{00AF}</mo></mover>"),
        "\\overline{ab}"
    );
}

#[test]
fn test_munder_low_line_is_underline() {
    // Authors write lower bounds as `munder` with a low-line char.
    assert_eq!(
        mathml_to_latex("<munder><mi>m</mi><mo>_</mo></munder>"),
        "\\underline{m}"
    );
}

#[test]
fn test_mover_with_content_script_keeps_overset() {
    assert_eq!(
        mathml_to_latex("<mover><mi>x</mi><mi>n</mi></mover>"),
        "\\overset{n}{x}"
    );
}

#[test]
fn test_literal_stretchy_brace_is_escaped() {
    // A `<mo>{</mo>` cases brace passed through raw changes LaTeX grouping
    // structure and leaves the formula unbalanced when its mate sits in
    // another table row.
    assert_eq!(mathml_to_latex("<mo>{</mo><mi>x</mi>"), "\\{x");
}

#[test]
fn test_literal_backslash_is_escaped() {
    // Set difference written as a raw backslash: `A\B` must not fuse into
    // an undefined control sequence `\B`.
    assert_eq!(mathml_to_latex("<mi>A</mi><mo>\\</mo><mi>B</mi>"), "A\\backslash B");
}

#[test]
fn test_mfenced_norm_delimiters() {
    assert_eq!(
        mathml_to_latex(r#"<mfenced open="&#x2225;" close="&#x2225;"><mi>x</mi></mfenced>"#),
        "\\left\\|x\\right\\|"
    );
}

#[test]
fn test_mfenced_angle_delimiters_do_not_glue() {
    assert_eq!(
        mathml_to_latex(r#"<mfenced open="&#x27E8;" close="&#x27E9;"><mi>A</mi></mfenced>"#),
        "\\left\\langle A\\right\\rangle "
    );
}

#[test]
fn test_mfenced_with_operator_children_drops_separators() {
    // `mfenced` abused as grouping: `(1 - x)` must not become `(1,-,x)`.
    assert_eq!(
        mathml_to_latex(r#"<mfenced><mn>1</mn><mo>-</mo><mi>x</mi></mfenced>"#),
        "\\left(1-x\\right)"
    );
}

#[test]
fn test_mtext_greek_moves_outside_text_group() {
    // `\Delta` is math-mode-only; inside `\text{}` it is undefined.
    assert_eq!(
        mathml_to_latex("<mtext>rate \u{0394}x</mtext>"),
        "\\text{rate }\\Delta \\text{x}"
    );
}

#[test]
fn test_mtext_escapes_structural_chars() {
    assert_eq!(mathml_to_latex("<mtext>m_{0} 50%</mtext>"), "\\text{m\\_\\{0\\} 50\\%}");
}

#[test]
fn test_braced_base_with_script_still_wraps() {
    // `{S_{\sigma }}_{1}` starts and ends with braces but is two atoms;
    // scripting it again without a wrap is a double subscript.
    assert_eq!(
        mathml_to_latex("<msub><msub><mrow><mi>S</mi><mi>b</mi></mrow><mn>1</mn></msub><mn>2</mn></msub>"),
        "{{Sb}_{1}}_{2}"
    );
}

#[test]
fn test_scripted_base_wraps_before_outer_script() {
    // `\lambda _{1}^{'}` scripted again must brace-wrap, or the outer
    // script produces a double superscript.
    assert_eq!(
        mathml_to_latex("<msup><msup><mi>\u{03BB}</mi><mn>1</mn></msup><mn>2</mn></msup>"),
        "{\\lambda ^{1}}^{2}"
    );
}

#[test]
fn test_empty_script_base_renders_as_empty_group() {
    // Tensor prescript markup: an empty base must yield `{}` so the script
    // cannot fuse onto the preceding atom as a double subscript.
    assert_eq!(
        mathml_to_latex("<msup><mi>T</mi><mi>\u{03BD}</mi></msup><msub><mrow/><mi>\u{03BD}</mi></msub>"),
        "T^{\\nu }{}_{\\nu }"
    );
}

#[test]
fn test_combining_overline_folds_into_bar() {
    // Identifiers carry combining marks (`U̅`); the raw mark is not a
    // KaTeX-valid accent.
    assert_eq!(mathml_to_latex("<mi>U\u{0305}</mi>"), "\\bar{U}");
    // A mark split into its own element applies to the previous atom.
    assert_eq!(mathml_to_latex("<mi>\u{03A3}</mi><mo>\u{0305}</mo>"), "\\bar{\\Sigma} ");
}

#[test]
fn test_munderover() {
    assert_eq!(
        mathml_to_latex("<munderover><mo>\u{2211}</mo><mi>i</mi><mi>n</mi></munderover>"),
        "\\overset{n}{\\underset{i}{\\sum }}"
    );
}

#[test]
fn test_mspace_renders_as_space() {
    assert_eq!(mathml_to_latex("<mrow><mi>a</mi><mspace/><mi>b</mi></mrow>"), "a b");
}

#[test]
fn test_mphantom() {
    assert_eq!(mathml_to_latex("<mphantom><mi>x</mi></mphantom>"), "\\phantom{x}");
}

#[test]
fn test_mtable_matrix() {
    let latex = mathml_to_latex(
        r#"<mtable>
            <mtr><mtd><mn>1</mn></mtd><mtd><mn>2</mn></mtd></mtr>
            <mtr><mtd><mn>3</mn></mtd><mtd><mn>4</mn></mtd></mtr>
        </mtable>"#,
    );
    assert_eq!(latex, "\\begin{matrix}1 & 2 \\\\ 3 & 4\\end{matrix}");
}

#[test]
fn test_unknown_element_degrades_to_text_content() {
    assert_eq!(mathml_to_latex("<mlongdiv><mn>42</mn></mlongdiv>"), "42");
}

/// A document that ships the author's TeX states the formula exactly, so it
/// beats reconstructing LaTeX from the presentation tree.
#[test]
fn test_tex_annotation_wins_over_the_presentation_tree() {
    let latex = mathml_to_latex(
        r#"<semantics>
            <mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow>
            <annotation encoding="application/x-tex">E = mc^2</annotation>
        </semantics>"#,
    );
    assert_eq!(latex, "E = mc^2");
}

/// Renderers wrap the expression in the style the surrounding document set.
#[test]
fn test_display_style_wrapper_comes_off() {
    let latex = mathml_to_latex(
        r#"<semantics>
            <mrow><mi>x</mi></mrow>
            <annotation encoding="application/x-tex">{\displaystyle x^{2}+1}</annotation>
        </semantics>"#,
    );
    assert_eq!(latex, "x^{2}+1");
}

/// A brace that closes before the end is part of the formula, so the
/// wrapper stays.
#[test]
fn test_partial_brace_group_keeps_the_wrapper() {
    assert_eq!(strip_style_wrapper("{\\displaystyle a} + b"), "{\\displaystyle a} + b");
}

/// An annotation in another notation is not TeX and must not leak.
#[test]
fn test_non_tex_annotation_still_renders_the_presentation_branch() {
    let latex = mathml_to_latex(
        r#"<semantics>
            <mrow><mi>E</mi><mo>=</mo><mi>m</mi></mrow>
            <annotation encoding="StarMath 5.0">E = m</annotation>
        </semantics>"#,
    );
    assert_eq!(latex, "E=m");
}

/// An empty annotation carries nothing, so the presentation tree stands.
#[test]
fn test_empty_tex_annotation_falls_back() {
    let latex = mathml_to_latex(
        r#"<semantics>
            <mrow><mi>a</mi><mo>+</mo><mi>b</mi></mrow>
            <annotation encoding="application/x-tex">   </annotation>
        </semantics>"#,
    );
    assert_eq!(latex, "a+b");
}

/// Content MathML states meaning rather than layout, so an `apply` tree
/// converts by operator.
#[test]
fn test_content_mathml_apply_converts_by_operator() {
    assert_eq!(mathml_to_latex("<apply><plus/><ci>a</ci><ci>b</ci></apply>"), "a+b");
    assert_eq!(mathml_to_latex("<apply><power/><ci>x</ci><cn>2</cn></apply>"), "x^{2}");
    assert_eq!(
        mathml_to_latex("<apply><root/><degree><cn>3</cn></degree><ci>x</ci></apply>"),
        "\\sqrt[3]{x}"
    );
}

#[test]
fn test_content_mathml_functions_and_relations() {
    assert_eq!(
        mathml_to_latex("<apply><eq/><ci>y</ci><apply><sin/><ci>x</ci></apply></apply>"),
        "y=\\sin\\left(x\\right)"
    );
}

/// A sum carries its bound variable and limits.
#[test]
fn test_content_mathml_sum_carries_limits() {
    let latex = mathml_to_latex(
        "<apply><sum/><bvar><ci>i</ci></bvar><lowlimit><cn>1</cn></lowlimit>             <uplimit><ci>n</ci></uplimit><ci>i</ci></apply>",
    );
    assert_eq!(latex, "\\sum_{i=1}^{n} i");
}

#[test]
fn test_content_mathml_matrix_and_piecewise() {
    let matrix = mathml_to_latex(
        "<matrix><matrixrow><cn>1</cn><cn>0</cn></matrixrow><matrixrow><cn>0</cn><cn>1</cn></matrixrow></matrix>",
    );
    assert_eq!(matrix, "\\begin{pmatrix}1 & 0 \\\\ 0 & 1\\end{pmatrix}");

    let cases = mathml_to_latex(
        "<piecewise><piece><cn>0</cn><apply><lt/><ci>x</ci><cn>0</cn></apply></piece>             <otherwise><ci>x</ci></otherwise></piecewise>",
    );
    assert!(cases.starts_with("\\begin{cases}"), "got: {cases}");
    assert!(cases.contains("\\text{otherwise}"), "got: {cases}");
}

/// An operator the mapping does not name still parses and still says what
/// the source said.
#[test]
fn test_unknown_content_operator_degrades() {
    assert_eq!(
        mathml_to_latex("<apply><wibble/><ci>a</ci></apply>"),
        "\\operatorname{wibble}\\left(a\\right)"
    );
}

/// A document that carries only the content branch has its meaning read from
/// `annotation-xml`, since the presentation side renders to nothing.
#[test]
fn test_content_annotation_is_used_when_presentation_is_empty() {
    let latex = mathml_to_latex(
        r#"<semantics><mrow/><annotation-xml encoding="MathML-Content">
            <apply><plus/><ci>a</ci><ci>b</ci></apply>
        </annotation-xml></semantics>"#,
    );
    assert_eq!(latex, "a+b");
}

/// A document with a working presentation branch keeps using it, so nothing
/// that already converted changes.
#[test]
fn test_presentation_branch_still_wins_over_content_annotation() {
    let latex = mathml_to_latex(
        r#"<semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi></mrow>
            <annotation-xml encoding="MathML-Content"><apply><plus/><ci>q</ci><ci>r</ci></apply></annotation-xml>
        </semantics>"#,
    );
    assert_eq!(latex, "E=m");
}

#[test]
fn test_semantics_renders_presentation_branch_only() {
    let latex = mathml_to_latex(
        r#"<semantics>
            <mrow><mi>E</mi><mo>=</mo><mi>m</mi></mrow>
            <annotation encoding="StarMath 5.0">E = m</annotation>
        </semantics>"#,
    );
    assert_eq!(latex, "E=m");
}

#[test]
fn test_nested_quadratic_formula() {
    // Uses a literal '±' (U+00B1) character, matching how the OMML test
    // suite embeds Unicode math symbols directly rather than as escapes
    // (escape sequences are not processed inside raw strings).
    let latex = mathml_to_latex(
        r#"<mi>x</mi><mo>=</mo>
        <mfrac>
            <mrow>
                <mo>-</mo><mi>b</mi><mo>±</mo>
                <msqrt>
                    <msup><mi>b</mi><mn>2</mn></msup>
                    <mo>-</mo><mn>4</mn><mi>a</mi><mi>c</mi>
                </msqrt>
            </mrow>
            <mrow><mn>2</mn><mi>a</mi></mrow>
        </mfrac>"#,
    );
    assert_eq!(latex, "x=\\frac{-b\\pm \\sqrt{b^{2}-4ac}}{2a}");
}

#[test]
fn test_formula_odt_fixture_shape() {
    // Mirrors the real embedded formula object in test_documents/odt/formula.odt:
    // E = m * c^2, wrapped in <semantics>/<annotation> with a StarMath fallback.
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <semantics>
                <mrow><mrow><mi>E</mi><mo stretchy="false">=</mo>
                <mrow><mi>m</mi><mo stretchy="false">⋅</mo>
                <msup><mi>c</mi><mn>2</mn></msup></mrow></mrow></mrow>
                <annotation encoding="StarMath 5.0">E = m cdot c^2</annotation>
            </semantics>
        </math>"#;
    let mut budget = SecurityBudget::with_defaults();
    let latex = convert_mathml_str_to_latex(xml, &mut budget).expect("conversion ok");
    assert_eq!(latex, "E=m\\cdot c^{2}");
}

#[test]
fn test_convert_mathml_node_to_latex_from_pre_parsed_node() {
    let xml = r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math>"#;
    let doc = roxmltree::Document::parse(xml).expect("parses");
    let mut budget = SecurityBudget::with_defaults();
    let latex = convert_mathml_node_to_latex(doc.root_element(), &mut budget).expect("conversion ok");
    assert_eq!(latex, "x");
}

#[test]
fn test_depth_failure_does_not_leak_mathml_budget_depth() {
    let limits = crate::extractors::security::SecurityLimits {
        max_nesting_depth: 1,
        max_xml_depth: 1,
        ..Default::default()
    };
    let mut budget = SecurityBudget::from_limits(&limits);
    let nested = roxmltree::Document::parse("<math><mrow><mi>x</mi></mrow></math>").expect("parse nested MathML");

    assert!(convert_mathml_node_to_latex(nested.root_element(), &mut budget).is_err());

    let shallow = roxmltree::Document::parse("<mi>x</mi>").expect("parse shallow MathML");
    assert_eq!(
        convert_mathml_node_to_latex(shallow.root_element(), &mut budget).expect("depth should be restored"),
        "x"
    );
}
