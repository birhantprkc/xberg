//! Test-only style inheritance resolution (`StyleCatalog::resolve_style`) and the property-merge
//! helpers it uses, exercised by `styles::tests`.

use super::*;

impl StyleCatalog {
    /// Resolve a style by walking its `basedOn` inheritance chain.
    ///
    /// The resolution order is:
    /// 1. Document defaults (`<w:docDefaults>`)
    /// 2. Base style chain (walking `basedOn` from root to leaf)
    /// 3. The style itself
    ///
    /// For `Option` fields, a child value of `Some(x)` overrides the parent.
    /// A value of `None` inherits from the parent. For boolean toggle properties,
    /// `Some(false)` explicitly disables the property.
    ///
    /// The chain depth is limited to 20 to prevent infinite loops from circular references.
    #[cfg(test)]
    pub(crate) fn resolve_style(&self, style_id: &str) -> ResolvedStyle {
        let mut resolved = ResolvedStyle {
            paragraph_properties: self.default_paragraph_properties.clone(),
            run_properties: self.default_run_properties.clone(),
        };

        let chain = self.collect_chain(style_id);

        for style_def in &chain {
            merge_paragraph_properties(&mut resolved.paragraph_properties, &style_def.paragraph_properties);
            merge_run_properties(&mut resolved.run_properties, &style_def.run_properties);
        }

        resolved
    }

    /// Collect the basedOn chain for a style, ordered from root ancestor to the style itself.
    ///
    /// Limited to 20 levels to prevent cycles.
    #[cfg(test)]
    fn collect_chain(&self, style_id: &str) -> Vec<&StyleDefinition> {
        const MAX_DEPTH: usize = 20;

        let mut chain = Vec::new();
        let mut current_id = Some(style_id.to_string());
        let mut visited = Vec::new();

        while let Some(id) = current_id {
            if visited.len() >= MAX_DEPTH {
                break;
            }
            if visited.contains(&id) {
                break;
            }

            if let Some(style_def) = self.styles.get(&id) {
                visited.push(id);
                chain.push(style_def);
                current_id = style_def.based_on.clone();
            } else {
                break;
            }
        }

        chain.reverse();
        chain
    }
}

/// Merge child run properties onto parent, where `Some` in child overrides parent.
#[cfg(test)]
fn merge_run_properties(base: &mut RunProperties, overlay: &RunProperties) {
    merge_run_text_properties(base, overlay);
    merge_run_appearance_properties(base, overlay);
}

/// Merge the text-identity half of `RunProperties` (weight, decoration, font selection).
#[cfg(test)]
fn merge_run_text_properties(base: &mut RunProperties, overlay: &RunProperties) {
    if overlay.bold.is_some() {
        base.bold = overlay.bold;
    }
    if overlay.italic.is_some() {
        base.italic = overlay.italic;
    }
    if overlay.underline.is_some() {
        base.underline = overlay.underline;
    }
    if overlay.strikethrough.is_some() {
        base.strikethrough = overlay.strikethrough;
    }
    if overlay.color.is_some() {
        base.color.clone_from(&overlay.color);
    }
    if overlay.font_size_half_points.is_some() {
        base.font_size_half_points = overlay.font_size_half_points;
    }
    if overlay.font_ascii.is_some() {
        base.font_ascii.clone_from(&overlay.font_ascii);
    }
    if overlay.font_ascii_theme.is_some() {
        base.font_ascii_theme.clone_from(&overlay.font_ascii_theme);
    }
    if overlay.vert_align.is_some() {
        base.vert_align.clone_from(&overlay.vert_align);
    }
    if overlay.font_h_ansi.is_some() {
        base.font_h_ansi.clone_from(&overlay.font_h_ansi);
    }
    if overlay.font_cs.is_some() {
        base.font_cs.clone_from(&overlay.font_cs);
    }
    if overlay.font_east_asia.is_some() {
        base.font_east_asia.clone_from(&overlay.font_east_asia);
    }
}

/// Merge the appearance half of `RunProperties` (highlight, toggles, spacing, theme color).
#[cfg(test)]
fn merge_run_appearance_properties(base: &mut RunProperties, overlay: &RunProperties) {
    if overlay.highlight.is_some() {
        base.highlight.clone_from(&overlay.highlight);
    }
    if overlay.caps.is_some() {
        base.caps = overlay.caps;
    }
    if overlay.small_caps.is_some() {
        base.small_caps = overlay.small_caps;
    }
    if overlay.shadow.is_some() {
        base.shadow = overlay.shadow;
    }
    if overlay.outline.is_some() {
        base.outline = overlay.outline;
    }
    if overlay.emboss.is_some() {
        base.emboss = overlay.emboss;
    }
    if overlay.imprint.is_some() {
        base.imprint = overlay.imprint;
    }
    if overlay.char_spacing.is_some() {
        base.char_spacing = overlay.char_spacing;
    }
    if overlay.position.is_some() {
        base.position = overlay.position;
    }
    if overlay.kern.is_some() {
        base.kern = overlay.kern;
    }
    if overlay.theme_color.is_some() {
        base.theme_color.clone_from(&overlay.theme_color);
        base.theme_tint = overlay.theme_tint.clone();
        base.theme_shade = overlay.theme_shade.clone();
    } else {
        if overlay.theme_tint.is_some() {
            base.theme_tint.clone_from(&overlay.theme_tint);
        }
        if overlay.theme_shade.is_some() {
            base.theme_shade.clone_from(&overlay.theme_shade);
        }
    }
}

/// Merge child paragraph properties onto parent, where `Some` in child overrides parent.
#[cfg(test)]
fn merge_paragraph_properties(base: &mut ParagraphProperties, overlay: &ParagraphProperties) {
    merge_paragraph_layout_properties(base, overlay);
    merge_paragraph_flow_properties(base, overlay);
}

/// Merge the layout half of `ParagraphProperties` (alignment, spacing, indentation, numbering).
#[cfg(test)]
fn merge_paragraph_layout_properties(base: &mut ParagraphProperties, overlay: &ParagraphProperties) {
    if overlay.alignment.is_some() {
        base.alignment.clone_from(&overlay.alignment);
    }
    if overlay.spacing_before.is_some() {
        base.spacing_before = overlay.spacing_before;
    }
    if overlay.spacing_after.is_some() {
        base.spacing_after = overlay.spacing_after;
    }
    if overlay.spacing_line.is_some() {
        base.spacing_line = overlay.spacing_line;
    }
    if overlay.spacing_line_rule.is_some() {
        base.spacing_line_rule.clone_from(&overlay.spacing_line_rule);
    }
    if overlay.indent_left.is_some() {
        base.indent_left = overlay.indent_left;
    }
    if overlay.indent_right.is_some() {
        base.indent_right = overlay.indent_right;
    }
    if overlay.indent_first_line.is_some() {
        base.indent_first_line = overlay.indent_first_line;
    }
    if overlay.indent_hanging.is_some() {
        base.indent_hanging = overlay.indent_hanging;
    }
    if overlay.outline_level.is_some() {
        base.outline_level = overlay.outline_level;
    }
    if overlay.numbering_id.is_some() {
        base.numbering_id = overlay.numbering_id;
    }
    if overlay.numbering_level.is_some() {
        base.numbering_level = overlay.numbering_level;
    }
}

/// Merge the flow/decoration half of `ParagraphProperties` (pagination, direction, shading,
/// borders).
#[cfg(test)]
fn merge_paragraph_flow_properties(base: &mut ParagraphProperties, overlay: &ParagraphProperties) {
    if overlay.keep_next.is_some() {
        base.keep_next = overlay.keep_next;
    }
    if overlay.keep_lines.is_some() {
        base.keep_lines = overlay.keep_lines;
    }
    if overlay.page_break_before.is_some() {
        base.page_break_before = overlay.page_break_before;
    }
    if overlay.widow_control.is_some() {
        base.widow_control = overlay.widow_control;
    }
    if overlay.suppress_auto_hyphens.is_some() {
        base.suppress_auto_hyphens = overlay.suppress_auto_hyphens;
    }
    if overlay.bidi.is_some() {
        base.bidi = overlay.bidi;
    }
    if overlay.shading_fill.is_some() {
        base.shading_fill.clone_from(&overlay.shading_fill);
    }
    if overlay.shading_val.is_some() {
        base.shading_val.clone_from(&overlay.shading_val);
    }
    if overlay.border_top.is_some() {
        base.border_top.clone_from(&overlay.border_top);
    }
    if overlay.border_bottom.is_some() {
        base.border_bottom.clone_from(&overlay.border_bottom);
    }
    if overlay.border_left.is_some() {
        base.border_left.clone_from(&overlay.border_left);
    }
    if overlay.border_right.is_some() {
        base.border_right.clone_from(&overlay.border_right);
    }
}
