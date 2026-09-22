//! Formula capture for XML formats.
//!
//! JATS and DocBook both wrap an equation in an element that may hold verbatim
//! TeX, a MathML subtree, or plain text. The capture below reads one such
//! element and returns its LaTeX, so each format states only the names of its
//! own child elements.

use crate::Result;
use crate::extraction::derive::strip_math_delimiters;
use crate::extractors::security::SecurityBudget;
use crate::utils::xml_utils::EntityReader;
use quick_xml::events::{BytesEnd, BytesStart, Event};

/// The child elements a format uses inside a formula.
pub(crate) struct FormulaElements<'a> {
    /// Element whose text is verbatim TeX and wins over MathML. JATS names it
    /// `tex-math`; DocBook names it `alt`.
    pub(crate) tex: &'a str,
    /// Element whose text is the equation number, which becomes a LaTeX `\tag`.
    /// `None` for a format that has no such element.
    pub(crate) label: Option<&'a str>,
}

/// Return the local part of a possibly prefixed XML qualified name.
fn local_name_of(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname)
}

/// Append a start tag to `buf` with the prefix stripped and namespace
/// declarations dropped, so the captured subtree parses without the
/// document's namespace context.
///
/// Attribute values are decoded and re-escaped, so single-quoted source
/// attributes with embedded double quotes stay well-formed. When two
/// attribute names collide after prefix stripping, the first one wins:
/// a duplicate attribute would make the captured XML unparseable.
fn write_start_tag(buf: &mut String, event: &BytesStart<'_>, self_closing: bool) {
    buf.push('<');
    buf.push_str(local_name_of(event.name().as_ref()));
    let mut written: Vec<String> = Vec::new();
    for attr in event.attributes().flatten() {
        let key = std::borrow::Cow::Borrowed(attr.key.as_ref());
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let local_key = key.rsplit(':').next().unwrap_or(&key).to_string();
        if written.contains(&local_key) {
            continue;
        }
        let raw = std::borrow::Cow::Borrowed(attr.value.as_ref());
        buf.push(' ');
        buf.push_str(&local_key);
        buf.push_str("=\"");
        match quick_xml::escape::unescape(&raw) {
            // Decoded value: re-escape for the double-quoted attribute.
            Ok(value) => buf.push_str(&quick_xml::escape::escape(value.as_ref())),
            // Undecodable reference: the raw bytes are already escaped, but a
            // literal quote from a single-quoted source attribute is not.
            Err(_) => buf.push_str(&raw.replace('"', "&quot;")),
        }
        buf.push('"');
        written.push(local_key);
    }
    if self_closing {
        buf.push('/');
    }
    buf.push('>');
}

/// Strip a full LaTeX document wrapper from verbatim TeX content.
///
/// PMC articles commonly ship `<tex-math>` as a complete compilable document
/// (`\documentclass...\begin{document}$$...$$\end{document}`); only the body
/// is the formula.
fn strip_latex_document_wrapper(tex: &str) -> &str {
    if !tex.contains("\\documentclass") {
        return tex;
    }
    let Some(start) = tex.find("\\begin{document}") else {
        return tex;
    };
    let body = &tex[start + "\\begin{document}".len()..];
    let body = match body.find("\\end{document}") {
        Some(end) => &body[..end],
        None => body,
    };
    body.trim()
}

/// Mutable state threaded through a formula subtree's event loop: everything captured so
/// far (verbatim TeX, MathML subtrees, the equation label, and flattened fallback text) plus
/// the flags tracking which of those a given event currently feeds. Grouped into one struct,
/// with one method per event kind, so the loop in [`extract_formula_latex`] stays a plain
/// dispatch and each event's handling gets its own nesting budget. ~keep
#[derive(Default)]
struct FormulaCapture {
    fallback_text: String,
    tex_math: String,
    label: String,
    mathml_xmls: Vec<String>,
    capture: Option<String>,
    capture_depth: usize,
    capture_in_alternatives: bool,
    alternatives_depth: usize,
    alternatives_math_seen: bool,
    in_tex_math: bool,
    in_label: bool,
}

impl FormulaCapture {
    /// Handle a start tag: either extend an in-progress `math` capture, begin a new one, or
    /// toggle one of the TeX/label/alternatives flags. Assumes the caller has already run
    /// `budget.enter()` and advanced the overall element depth.
    fn handle_start(
        &mut self,
        s: &BytesStart<'_>,
        names: &FormulaElements<'_>,
        budget: &mut SecurityBudget,
    ) -> Result<()> {
        let name = s.name();
        let local = local_name_of(name.as_ref());
        if let Some(buf) = self.capture.as_mut() {
            self.capture_depth += 1;
            let before = buf.len();
            write_start_tag(buf, s, false);
            budget.account_text(buf.len() - before)?;
        } else if local == "math" {
            let mut buf = String::new();
            write_start_tag(&mut buf, s, false);
            budget.account_text(buf.len())?;
            self.capture = Some(buf);
            self.capture_depth = 1;
            self.capture_in_alternatives = self.alternatives_depth > 0;
        } else if local == "alternatives" {
            self.alternatives_depth += 1;
        } else if local == names.tex {
            self.in_tex_math = true;
        } else if names.label.is_some_and(|name| local == name) {
            self.in_label = true;
        }
        Ok(())
    }

    /// Handle a self-closing tag: append it to an in-progress `math` capture, if any.
    fn handle_empty(&mut self, s: &BytesStart<'_>, budget: &mut SecurityBudget) -> Result<()> {
        if let Some(buf) = self.capture.as_mut() {
            let before = buf.len();
            write_start_tag(buf, s, true);
            budget.account_text(buf.len() - before)?;
        }
        Ok(())
    }

    /// Handle an end tag: close an in-progress `math` capture (recording it once its depth
    /// unwinds to zero) or, outside a capture, clear whichever TeX/label/alternatives flag it
    /// closes. Assumes the caller has already run `budget.leave()`.
    fn handle_end(&mut self, e: &BytesEnd<'_>, names: &FormulaElements<'_>) {
        let Some(buf) = self.capture.as_mut() else {
            let name = e.name();
            let local = local_name_of(name.as_ref());
            if local == names.tex {
                self.in_tex_math = false;
            } else if names.label.is_some_and(|name| local == name) {
                self.in_label = false;
            } else if local == "alternatives" {
                self.alternatives_depth = self.alternatives_depth.saturating_sub(1);
            }
            return;
        };
        buf.push_str("</");
        buf.push_str(local_name_of(e.name().as_ref()));
        buf.push('>');
        self.capture_depth -= 1;
        if self.capture_depth == 0
            && let Some(xml) = self.capture.take()
        {
            self.record_captured_math(xml);
        }
    }

    /// Record a finished `math` capture. Inside `<alternatives>` every `math` sibling is one
    /// more representation of the SAME formula: keep the first. Outside, each sibling is its
    /// own equation.
    fn record_captured_math(&mut self, xml: String) {
        if !self.capture_in_alternatives {
            self.mathml_xmls.push(xml);
        } else if !self.alternatives_math_seen {
            self.alternatives_math_seen = true;
            self.mathml_xmls.push(xml);
        }
    }

    /// Route decoded text content to the capture, the TeX buffer, the label, or the fallback
    /// text, whichever is currently active.
    fn handle_text(&mut self, decoded: &str) {
        if let Some(buf) = self.capture.as_mut() {
            buf.push_str(&quick_xml::escape::escape(decoded));
        } else if self.in_tex_math {
            self.tex_math.push_str(decoded);
        } else if self.in_label {
            if !self.label.is_empty() {
                self.label.push(' ');
            }
            self.label.push_str(decoded);
        } else {
            self.fallback_text.push_str(decoded);
            self.fallback_text.push(' ');
        }
    }

    /// Route decoded CDATA content the same way as text, except TeX wins over an in-progress
    /// `math` capture (CDATA is how some sources wrap verbatim TeX containing `<`/`&`).
    fn handle_cdata(&mut self, decoded: &str) {
        if self.in_tex_math {
            self.tex_math.push_str(decoded);
        } else if let Some(buf) = self.capture.as_mut() {
            buf.push_str(&quick_xml::escape::escape(decoded));
        } else {
            self.fallback_text.push_str(decoded);
            self.fallback_text.push(' ');
        }
    }
}

/// Extract the LaTeX for a formula subtree.
///
/// The caller has consumed the formula start tag. The preference order is:
/// the TeX element's text verbatim, then the `math` subtree converted with the
/// shared MathML converter, then the flattened text content.
pub(crate) fn extract_formula_latex(
    reader: &mut EntityReader<'_>,
    budget: &mut SecurityBudget,
    names: &FormulaElements<'_>,
) -> Result<String> {
    let mut state = FormulaCapture::default();
    let mut depth = 0usize;

    loop {
        budget.step()?;
        match reader.read_event() {
            Ok(Event::Start(s)) => {
                budget.enter()?;
                depth += 1;
                state.handle_start(&s, names, budget)?;
            }
            Ok(Event::Empty(s)) => {
                state.handle_empty(&s, budget)?;
            }
            Ok(Event::End(e)) => {
                budget.leave();
                state.handle_end(&e, names);
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Text(t)) => {
                let decoded = t.as_ref().to_string();
                if decoded.trim().is_empty() {
                    continue;
                }
                budget.check_entity(&decoded)?;
                budget.account_text(decoded.len())?;
                state.handle_text(&decoded);
            }
            Ok(Event::CData(t)) => {
                let decoded = t.as_ref().to_string();
                if decoded.trim().is_empty() {
                    continue;
                }
                budget.check_entity(&decoded)?;
                budget.account_text(decoded.len())?;
                state.handle_cdata(&decoded);
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(crate::error::XbergError::parsing(format!("XML parsing error: {}", e)));
            }
            _ => {}
        }
    }

    finalize_formula_latex(state, budget)
}

/// Resolve a finished [`FormulaCapture`] into LaTeX: verbatim TeX wins, then the captured
/// `math` subtree(s) converted through the shared MathML converter, then flattened fallback
/// text. A non-empty equation label becomes a LaTeX `\tag` on either of the first two.
fn finalize_formula_latex(state: FormulaCapture, budget: &mut SecurityBudget) -> Result<String> {
    let FormulaCapture {
        mut fallback_text,
        tex_math,
        label,
        mathml_xmls,
        ..
    } = state;

    // An equation label (`<label>1.1</label>`) becomes a LaTeX `\tag` so the
    // equation number survives the conversion. `\tag` renders inside parens,
    // so a source label that already carries them (`(1)`) sheds one pair —
    // otherwise the equation number displays as `((1))`.
    let with_tag = |latex: &str| -> String {
        let mut label = label.trim();
        if label.len() >= 2 && label.starts_with('(') && label.ends_with(')') {
            label = label[1..label.len() - 1].trim();
        }
        let label: String = label.chars().filter(|c| *c != '{' && *c != '}').collect();
        if label.is_empty() {
            latex.to_string()
        } else {
            format!("{latex} \\tag{{{label}}}")
        }
    };

    let tex = strip_math_delimiters(strip_latex_document_wrapper(tex_math.trim()));
    if !tex.is_empty() {
        return Ok(with_tag(tex));
    }
    if !mathml_xmls.is_empty() {
        let mut parts: Vec<String> = Vec::new();
        for xml in &mathml_xmls {
            let latex = crate::extraction::mathml::convert_mathml_str_to_latex(xml, budget)?;
            if !latex.trim().is_empty() {
                parts.push(latex.trim().to_string());
            }
        }
        if !parts.is_empty() {
            return Ok(with_tag(&parts.join(" \\\\ ")));
        }
    }
    if !label.trim().is_empty() {
        fallback_text = format!("{} {}", label.trim(), fallback_text);
    }
    Ok(fallback_text.trim().to_string())
}
