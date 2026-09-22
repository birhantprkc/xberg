//! LaTeX rendering for a collected [`super::MmlNode`] tree: the second half of MathML
//! conversion, split out from `mathml.rs` once the collection phase (XML subtree ->
//! `MmlNode`) and this rendering phase (`MmlNode` -> LaTeX string) together made the parent
//! file too long. Only [`render_nodes`] and [`render_node`] are called from outside this
//! module; everything else here is a private helper of the rendering phase.

use super::MmlNode;
use crate::extraction::math_symbols::render_run_text;

/// Render a slice of `MmlNode`s to LaTeX, concatenated with no separators.
pub(super) fn render_nodes(nodes: &[MmlNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        render_node(node, &mut out);
    }
    out
}

/// Render a single `MmlNode` to LaTeX, appending to `out`.
pub(super) fn render_node(node: &MmlNode, out: &mut String) {
    match node {
        MmlNode::Verbatim(latex) => out.push_str(latex),
        MmlNode::Run(text) => render_run_text(text, out),
        MmlNode::Text(text) => render_text_content(text, out),
        MmlNode::Space => out.push(' '),
        MmlNode::Frac { num, den } => {
            out.push_str("\\frac{");
            render_node(num, out);
            out.push_str("}{");
            render_node(den, out);
            out.push('}');
        }
        MmlNode::Sup { base, sup } => {
            render_arg(base, out);
            out.push_str("^{");
            render_node(sup, out);
            out.push('}');
        }
        MmlNode::Sub { base, sub } => {
            render_arg(base, out);
            out.push_str("_{");
            render_node(sub, out);
            out.push('}');
        }
        MmlNode::SubSup { base, sub, sup } => {
            render_arg(base, out);
            out.push_str("_{");
            render_node(sub, out);
            out.push_str("}^{");
            render_node(sup, out);
            out.push('}');
        }
        MmlNode::Sqrt { body } => {
            out.push_str("\\sqrt{");
            render_node(body, out);
            out.push('}');
        }
        MmlNode::Root { body, index } => {
            out.push_str("\\sqrt[");
            render_node(index, out);
            out.push_str("]{");
            render_node(body, out);
            out.push('}');
        }
        MmlNode::Fenced {
            open,
            close,
            sep,
            elements,
        } => render_fenced(open, close, sep, elements, out),
        MmlNode::Under { base, under } => render_under(base, under, out),
        MmlNode::Over { base, over } => render_over(base, over, out),
        MmlNode::UnderOver { base, under, over } => render_under_over(base, under, over, out),
        MmlNode::Phantom { body } => {
            out.push_str("\\phantom{");
            render_node(body, out);
            out.push('}');
        }
        MmlNode::Table { rows } => render_table(rows, out),
        MmlNode::Group { children } => out.push_str(&render_nodes(children)),
    }
}

/// Render an underscript: a recognized under-script command (e.g. `\underline`) applied
/// directly, or `\underset{under}{base}` when `under` has no such command.
fn render_under(base: &MmlNode, under: &MmlNode, out: &mut String) {
    if let Some(cmd) = under_script_command(under) {
        out.push_str(cmd);
        out.push('{');
        render_node(base, out);
        out.push('}');
    } else {
        out.push_str("\\underset{");
        render_node(under, out);
        out.push_str("}{");
        render_node(base, out);
        out.push('}');
    }
}

/// Render an overscript: a recognized over-script command (e.g. `\vec`, `\hat`) applied
/// directly, or `\overset{over}{base}` when `over` has no such command.
fn render_over(base: &MmlNode, over: &MmlNode, out: &mut String) {
    if let Some(cmd) = over_script_command(over, base) {
        out.push_str(cmd);
        out.push('{');
        render_node(base, out);
        out.push('}');
    } else {
        out.push_str("\\overset{");
        render_node(over, out);
        out.push_str("}{");
        render_node(base, out);
        out.push('}');
    }
}

/// Render a combined under+overscript as `\overset{over}{\underset{under}{base}}`.
fn render_under_over(base: &MmlNode, under: &MmlNode, over: &MmlNode, out: &mut String) {
    out.push_str("\\overset{");
    render_node(over, out);
    out.push_str("}{\\underset{");
    render_node(under, out);
    out.push_str("}{");
    render_node(base, out);
    out.push_str("}}");
}

/// Render an `mfenced` node's open/close delimiters and separator-joined elements.
///
/// Authors use `mfenced` as plain grouping with operators as direct children; inserting the
/// spec-default comma separators there turns `(1 - x)` into `(1,-,x)`. Suppress separators when
/// any child is itself an infix operator.
fn render_fenced(open: &str, close: &str, sep: &str, elements: &[MmlNode], out: &mut String) {
    let sep = if elements.iter().any(is_operator_child) {
        ""
    } else {
        sep
    };
    let (left, right) = (fence_chr_to_latex(open), fence_chr_to_latex(close));
    match (left, right) {
        (Some(left), Some(right)) => {
            out.push_str("\\left");
            out.push_str(left);
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                render_node(elem, out);
            }
            out.push_str("\\right");
            out.push_str(right);
        }
        // A fence char LaTeX cannot use after `\left`: emit the fences
        // as plain glyphs instead of producing an unparseable string.
        _ => {
            render_run_text(open, out);
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                render_node(elem, out);
            }
            render_run_text(close, out);
        }
    }
}

/// Render an `mtable` node as a LaTeX `matrix` environment, rows separated by `\\` and cells by
/// `&`.
fn render_table(rows: &[Vec<MmlNode>], out: &mut String) {
    out.push_str("\\begin{matrix}");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(" \\\\ ");
        }
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                out.push_str(" & ");
            }
            render_node(cell, out);
        }
    }
    out.push_str("\\end{matrix}");
}

/// True when `s` is exactly one balanced brace group (`{...}`): the opening
/// brace's closer is the final character. `{a}^{b}` starts with `{` and ends
/// with `}` but is two atoms — treating it as pre-braced produces double
/// scripts when another script attaches.
fn is_single_brace_group(s: &str) -> bool {
    if !s.starts_with('{') || !s.ends_with('}') {
        return false;
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i == s.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

/// Render `mtext` content. Plain text goes inside `\text{...}` with text-mode
/// escaping; characters that map to math commands (Greek letters, operators)
/// are emitted *outside* the `\text` group, because commands like `\Delta` are
/// math-mode-only and fail inside `\text{}`.
fn render_text_content(text: &str, out: &mut String) {
    let mut in_text = false;
    for ch in text.chars() {
        if let Some(latex) = crate::extraction::math_symbols::unicode_to_latex(ch) {
            if in_text {
                out.push('}');
                in_text = false;
            }
            out.push_str(latex);
            continue;
        }
        if !in_text {
            out.push_str("\\text{");
            in_text = true;
        }
        match ch {
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '#' => out.push_str("\\#"),
            '$' => out.push_str("\\$"),
            '_' => out.push_str("\\_"),
            '\\' => out.push_str("\\textbackslash "),
            '^' => out.push_str("\\textasciicircum "),
            '~' => out.push_str("\\textasciitilde "),
            _ => out.push(ch),
        }
    }
    if in_text {
        out.push('}');
    }
}

/// The raw script text of an accent-like script node (`mo`/`mi` leaf, possibly
/// inside grouping), or `None` when the script is real content.
fn script_leaf_text(node: &MmlNode) -> Option<&str> {
    match node {
        MmlNode::Run(text) => Some(text.trim()),
        MmlNode::Group { children } if children.len() == 1 => script_leaf_text(&children[0]),
        _ => None,
    }
}

/// True when the base renders to a single glyph (possibly one LaTeX command),
/// used to pick `\bar`/`\vec` over `\overline`/`\overrightarrow`.
fn base_is_single_glyph(base: &MmlNode) -> bool {
    let mut rendered = String::new();
    render_node(base, &mut rendered);
    let t = rendered.trim();
    t.chars().count() == 1 || (t.starts_with('\\') && t[1..].chars().all(|c| c.is_ascii_alphabetic()))
}

/// Map an `mover` script char to a LaTeX accent command. MathML sources write
/// accents as literal combining/spacing characters (`<mover><mi>x</mi>
/// <mo>^</mo></mover>`); `\overset{^}{x}` is not valid LaTeX (bare `^` needs a
/// group), so these must become accent macros.
fn over_script_command(over: &MmlNode, base: &MmlNode) -> Option<&'static str> {
    match script_leaf_text(over)? {
        "^" | "\u{02C6}" | "\u{0302}" => Some("\\hat"),
        "~" | "\u{02DC}" | "\u{0303}" | "\u{223C}" => Some("\\tilde"),
        "\u{02D9}" | "\u{0307}" => Some("\\dot"),
        "\u{00A8}" | "\u{0308}" => Some("\\ddot"),
        "\u{00AF}" | "\u{203E}" | "\u{0304}" | "\u{0305}" => Some(if base_is_single_glyph(base) {
            "\\bar"
        } else {
            "\\overline"
        }),
        "\u{2192}" | "\u{20D7}" => Some(if base_is_single_glyph(base) {
            "\\vec"
        } else {
            "\\overrightarrow"
        }),
        "\u{02C7}" | "\u{030C}" => Some("\\check"),
        "\u{02D8}" | "\u{0306}" => Some("\\breve"),
        "\u{00B4}" | "\u{0301}" => Some("\\acute"),
        "`" | "\u{0300}" => Some("\\grave"),
        "\u{02DA}" | "\u{030A}" => Some("\\mathring"),
        "\u{23DE}" => Some("\\overbrace"),
        _ => None,
    }
}

/// Map an `munder` script char to a LaTeX command, like [`over_script_command`].
fn under_script_command(under: &MmlNode) -> Option<&'static str> {
    match script_leaf_text(under)? {
        "_" | "\u{0332}" | "\u{02CD}" | "\u{00AF}" | "\u{203E}" => Some("\\underline"),
        "\u{23DF}" => Some("\\underbrace"),
        _ => None,
    }
}

/// Render an argument (sup/sub base), wrapping in braces unless it is a single
/// atom (one character, one LaTeX command, or one brace group).
///
/// A compound base that already carries a script (`\lambda _{1}^{'}`) MUST be
/// wrapped, or attaching the outer script produces a double superscript. An
/// empty base (script-only markup like tensor `{}_{,\nu}`) renders as `{}` so
/// the script cannot fuse onto the preceding atom.
fn render_arg(node: &MmlNode, out: &mut String) {
    let mut rendered = String::new();
    render_node(node, &mut rendered);
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        out.push_str("{}");
        return;
    }
    let single_char = trimmed.chars().count() == 1;
    let single_command =
        trimmed.starts_with('\\') && trimmed.len() > 1 && trimmed[1..].chars().all(|c| c.is_ascii_alphabetic());
    if single_char || single_command || is_single_brace_group(trimmed) {
        out.push_str(&rendered);
    } else {
        out.push('{');
        out.push_str(trimmed);
        out.push('}');
    }
}

/// True when a fenced child renders to a bare infix operator, meaning the
/// `mfenced` is grouping an expression, not listing arguments.
fn is_operator_child(node: &MmlNode) -> bool {
    let MmlNode::Run(text) = node else { return false };
    matches!(
        text.trim(),
        "+" | "-" | "\u{2212}" | "=" | "\u{00B1}" | "\u{00D7}" | "\u{22C5}" | "/" | "<" | ">" | "\u{2264}" | "\u{2265}"
    )
}

/// Map an `mfenced` open/close character to a LaTeX delimiter valid after
/// `\left`/`\right`, or `None` for characters LaTeX cannot use there.
/// Word-form commands carry a trailing space so following content never glues
/// onto the control word (`\langle A`, not `\langleA`).
fn fence_chr_to_latex(chr: &str) -> Option<&'static str> {
    match chr {
        "(" => Some("("),
        ")" => Some(")"),
        "[" => Some("["),
        "]" => Some("]"),
        "{" => Some("\\{"),
        "}" => Some("\\}"),
        "|" | "\u{2223}" => Some("|"),
        "\u{2016}" | "\u{2225}" => Some("\\|"),
        "\u{2329}" | "\u{27E8}" => Some("\\langle "),
        "\u{232A}" | "\u{27E9}" => Some("\\rangle "),
        "\u{230A}" => Some("\\lfloor "),
        "\u{230B}" => Some("\\rfloor "),
        "\u{2308}" => Some("\\lceil "),
        "\u{2309}" => Some("\\rceil "),
        "/" => Some("/"),
        "\\" => Some("\\backslash "),
        "" => Some("."),
        _ => None,
    }
}
