//! XY-Cut recursive spatial partitioning for multi-column text layout.
//!
//! This module implements the XY-Cut algorithm per PDF Spec Section 9.4 for
//! recursive geometric analysis without semantic heuristics. Uses projection
//! profiles to detect column boundaries in complex layouts.
//!
//! Per ISO 32000-1:2008:
//! - Section 9.4: Text Objects and coordinates
//! - Section 14.7: Logical Structure (prefers structure tree when available)
//!
//! # Algorithm Overview
//!
//! 1. Compute horizontal projection (white space density across X)
//! 2. Find valleys (gaps) where density < threshold
//! 3. Split region at widest valley (vertical line)
//! 4. Recursively partition left and right sub-regions
//! 5. Alternate to vertical projection if no horizontal valleys found
//! 6. Base case: Sort spans top-to-bottom, left-to-right
//!
//! # Performance
//!
//! Typical newspaper page: ~100 spans, < 5ms processing time
//! Recursive depth: O(log n) for balanced columns

// TODO(xberg-io/xberg#1567): 4 cyclomatic-complexity and 13 size/complexity findings
// in this file, currently excluded via the quality-debt baseline in alef.toml. Splitting
// these needs compiler-in-the-loop verification, not a mechanical pass. Delete this
// note and the file's baseline entry together once it goes green. Help wanted.

use super::{ReadingOrderContext, ReadingOrderStrategy};
use crate::error::Result;
use crate::geometry::Rect;
use crate::layout::TextSpan;
use crate::pipeline::{OrderedTextSpan, ReadingOrderInfo};

/// Maximum density-array length for XY-cut projection profiles.
///
/// A normal PDF page is at most a few thousand points wide/tall. This limit of
/// 100 000 bins is generous (≈ 33× a 3000-point A0 page) while being small
/// enough to never cause an allocation problem. Spans whose bounding-box span
/// exceeds this limit are the result of a degenerate CTM; returning `None` from
/// the projection safely skips the split instead of attempting a multi-terabyte
/// allocation that would abort the process via `handle_alloc_error`.
const MAX_PROJECTION_SIZE: usize = 100_000;

/// Coarse classification of a region for the multi-column-prose
/// fix. Used to gate the tight-gutter cut: tight cuts are only accepted on
/// regions that *positively* identify as prose, so the same XY-cut recursion
/// no longer corrupts table cells (the lesson — see lines 73–101).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    /// Tall stack of wide lines OR tall stack of half-column lines with
    /// substantial content per line. Safe to apply tight-gutter cuts.
    Prose,
    /// Short cells in a grid (mean characters per line < 8). Tight cuts
    /// here corrupt cell ordering — the canonical google_doc population
    /// table that reverted two earlier attempts is the prototype.
    Table,
    /// Anything else — too few lines, mixed shapes, decorative regions.
    /// Default to the behaviour (no tight cut).
    Mixed,
}

/// Contiguous run of bold-or-larger-font spans spanning ≥ 2 visual lines
/// that the XY-cut splitter must treat as an atomic block. Built by
/// `find_heading_runs` BEFORE recursive partitioning,
/// then substituted into the partition input as a single wide synthetic
/// span so cluster-detection / valley-finding can't drive a vertical
/// cut THROUGH a wrapped heading.
///
/// After partition completes, `expand_blocks` projects the synthetic
/// placeholder back into its constituent original spans, preserving
/// each span's per-glyph metadata for downstream consumers
/// (markdown converter heading-level inference, layout-preserving
/// DOCX export, etc.).
#[derive(Debug, Clone)]
struct HeadingRun {
    /// Indices into the original `&[TextSpan]` slice, in reading order
    /// (top-to-bottom, left-to-right within a line).
    span_indices: Vec<usize>,
    /// Union of the constituent spans' bboxes. Substituted for each
    /// individual bbox during partition so the heading appears as one
    /// wide bbox.
    combined_bbox: Rect,
}

/// Union of the bboxes of `spans[indices]`. Empty index list yields a
/// zero-sized rect at the origin (never built in practice — guarded by
/// the caller).
fn union_bboxes(spans: &[TextSpan], indices: &[usize]) -> Rect {
    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;
    for &i in indices {
        let b = spans[i].bbox;
        x_min = x_min.min(b.left());
        x_max = x_max.max(b.right());
        y_min = y_min.min(b.top());
        y_max = y_max.max(b.bottom());
    }
    if x_min == f32::MAX {
        return Rect::default();
    }
    Rect::from_points(x_min, y_min, x_max, y_max)
}

/// XY-Cut recursive spatial partitioning strategy.
///
/// Detects columns using projection profiles and white space analysis.
/// Suitable for newspapers, academic papers, and multi-column layouts.
pub struct XYCutStrategy {
    /// Minimum number of spans in a region before attempting split (default: 5).
    /// Prevents excessive recursion on small regions.
    pub min_spans_for_split: usize,

    /// Valley threshold as fraction of peak projection density (default: 0.3).
    /// Lower values detect narrower gutters, higher values only detect wide gaps.
    pub valley_threshold: f32,

    /// Minimum valley width in points (default: 15.0).
    /// Prevents detecting single-character gaps as column boundaries.
    pub min_valley_width: f32,

    /// Enable horizontal partitioning first, fallback to vertical (default: true).
    ///
    /// Per PDF Spec ISO 32000-1:2008 §14.8.4 (Logical Structure reading order),
    /// column detection is the primary purpose of XY-Cut — horizontal-first
    /// (vertical cut line) splits columns before rows, matching Western
    /// top-down-left-to-right reading order in multi-column documents.
    /// Callers with row-dominant layouts can override via
    /// `with_prefer_horizontal(false)`.
    pub prefer_horizontal: bool,
}

/// Cap on `partition_indexed` recursion depth. Real layouts nest only a few
/// splits deep; this bound only fires on the singleton-peel pathology (many
/// distinct-Y header/footer strips) where unbounded depth is O(n² log n). Set
/// high enough that no real document reaches it.
const MAX_PARTITION_DEPTH: u32 = 64;

impl Default for XYCutStrategy {
    fn default() -> Self {
        Self {
            min_spans_for_split: 5,
            valley_threshold: 0.3,
            // 15pt. A fix for multi-column prose interleaving was attempted
            // TWICE and REVERTED both times — the 70-PDF sweep caught data
            // corruption in the google_doc population table's digits
            // ("273.879.7501" -> "1273.879.750") each time:
            //
            //   Attempt 1 — lower min_valley_width 15 -> 12 so the tight
            //   ~12pt two-column gutter is detected. Also split the
            //   table's ~12pt inter-cell gaps -> reordered digits.
            //
            //   Attempt 2 — a structural find_two_column_prose_split
            //   (exactly-two recurring left-edge clusters, wide columns,
            //   clean gutter) tried before the single-column check. It
            //   never fired on the target page's WHOLE extent (three left-edge
            //   clusters: full-width intro/footer @60 + left @82 + right
            //   @312, because is_single_column blocks band separation
            //   first), yet it DID fire on a 2-column sub-region of the
            //   google_doc table and reordered cells.
            //
            // Root cause: the same XY-Cut machinery orders both
            // prose-columns and table-cells. Any sensitivity increase
            // that catches tight 2-column prose also splits
            // table cells and corrupts data. A correct fix needs a
            // real table-vs-prose classifier (column cells are short
            // values; prose columns are tall stacks of wide lines) AND
            // recursive band-separation of full-width header/footer rows
            // before column detection — a substantial XY-Cut redesign,
            // validated against the full CI corpus, not a local tweak. ~keep
            min_valley_width: 15.0,
            prefer_horizontal: true,
        }
    }
}

impl XYCutStrategy {
    /// Create a new XY-Cut strategy with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom valley threshold (0.0-1.0).
    pub fn with_valley_threshold(mut self, threshold: f32) -> Self {
        self.valley_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Create with custom minimum valley width.
    pub fn with_min_valley_width(mut self, width: f32) -> Self {
        self.min_valley_width = width.max(1.0);
        self
    }

    /// Enable or disable horizontal partitioning first preference.
    pub fn with_prefer_horizontal(mut self, prefer: bool) -> Self {
        self.prefer_horizontal = prefer;
        self
    }

    /// Core recursive partitioning algorithm.
    ///
    /// Public for use by MarkdownConverter's ColumnAware reading order mode.
    ///
    /// Runs a pre-pass that detects multi-line heading runs
    /// (bold or larger-than-body font, ≥ 2 wrapped lines with matching
    /// X-extent) and locks them as atomic blocks the recursive splitter
    /// cannot split. Without this, a wrapped heading whose tail lines
    /// Y-overlap with adjacent-column dense content (table caption, table
    /// row, image label) gets bucketed across columns: line 1 glued to the
    /// body paragraph, line 2..N orphaned into the wrong block — and the
    /// markdown converter then promotes the orphan tail to a phantom
    /// heading (`### …`) in the wrong location.
    pub fn partition_region(&self, spans: &[TextSpan]) -> Vec<Vec<TextSpan>> {
        let heading_runs = self.find_heading_runs(spans);
        if heading_runs.is_empty() {
            // Hot path: no headings found, skip the synthesize/expand
            // pair entirely so the cost is bounded to one O(n log n) sort
            // inside find_heading_runs. ~keep
            let indices: Vec<usize> = (0..spans.len()).collect();
            let index_groups = self.partition_indexed(spans, &indices);
            return index_groups
                .into_iter()
                .map(|group| group.into_iter().map(|i| spans[i].clone()).collect())
                .collect();
        }

        let (synthetic, synthetic_origin) = self.synthesize_for_partition(spans, &heading_runs);
        let synth_indices: Vec<usize> = (0..synthetic.len()).collect();
        let synth_groups = self.partition_indexed(&synthetic, &synth_indices);

        self.expand_blocks(synth_groups, spans, &synthetic_origin)
    }

    /// Detect contiguous bold/large-font runs that span ≥ 2 lines with
    /// matching X-extent (i.e. wrapped subsection headings).
    ///
    /// Per the fix-543 plan §A.2: two adjacent spans (in reading
    /// order) are considered to belong to the same heading run when
    /// ALL of the following hold:
    ///
    /// 1. Both are heading-like (bold, OR font_size > median × 1.15).
    /// 2. Same font_size (within 0.5 pt epsilon).
    /// 3. Same bold flag.
    /// 4. Next span's left edge is within `[prev.left, prev.left + 6pt]`
    ///    (wrapped heading lines often re-indent by up to ~6pt).
    /// 5. Next span sits ≤ 1.5 × line-height below the previous span
    ///    (a single-line gap; double-line gaps are paragraph breaks).
    ///
    /// `median_font_size` is computed across non-bold spans so heavy
    /// bold runs don't bias the body-size estimate upward.
    fn find_heading_runs(&self, spans: &[TextSpan]) -> Vec<HeadingRun> {
        if spans.len() < 2 {
            return Vec::new();
        }

        // Median body font size from NON-bold spans only. Bold spans
        // typically sit at heading sizes (bigger than body), so including
        // them biases the median high and we'd miss bold headings whose
        // size sits between body and the heavier weight tier. ~keep
        let mut non_bold_sizes: Vec<f32> = spans
            .iter()
            .filter(|s| !s.font_weight.is_bold())
            .map(|s| s.font_size)
            .filter(|&sz| sz > 0.0)
            .collect();
        let median_body = if non_bold_sizes.is_empty() {
            let mut sizes: Vec<f32> = spans.iter().map(|s| s.font_size).filter(|&sz| sz > 0.0).collect();
            if sizes.is_empty() {
                return Vec::new();
            }
            sizes.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            sizes[sizes.len() / 2]
        } else {
            non_bold_sizes.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            non_bold_sizes[non_bold_sizes.len() / 2]
        };
        let heading_size_floor = median_body * 1.15;

        let is_heading_like = |s: &TextSpan| -> bool { s.font_weight.is_bold() || s.font_size > heading_size_floor };

        // Sort indices by reading order (top of page first; Rect::top()
        // is the SMALLER Y of the normalized rect — see comment at
        // line ~885 — so larger Y = higher on page in PDF coords;
        // we want DESCENDING Y here). ~keep
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&a, &b| {
            let y_cmp = crate::utils::safe_float_cmp(spans[b].bbox.top(), spans[a].bbox.top());
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            crate::utils::safe_float_cmp(spans[a].bbox.left(), spans[b].bbox.left())
        });

        // Cluster reading-order-adjacent heading-like spans into runs.
        // The same line may carry multiple bold spans (one per Tj
        // segment); we collapse runs across lines, not within a line. ~keep
        let indent_tolerance = 6.0_f32;
        let font_eps = 0.5_f32;
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();

        for &idx in &order {
            let span = &spans[idx];

            // A whitespace-only span carries no ink, so it must not break an
            // otherwise-continuous heading run. A `TJ` array's tab kern
            // (`[(3.)-1329.5(Title)] TJ`) turns into exactly such a span,
            // always `FontWeight::Normal` regardless of the surrounding
            // bold context (see `advance.rs`'s synthetic-space
            // construction) — so `is_heading_like` below would reject it
            // and sever the run at the marker/title boundary, splitting a
            // numbered heading's marker from its own title into two
            // separate (non-heading, single-span) fragments while the
            // wrapped continuation line still merges into one. The
            // narrower fragment's union bbox then no longer covers the
            // marker's column position, letting an unrelated span at the
            // marker's row/column slot in between during partitioning.
            // Skipping (not breaking, not joining) the whitespace span
            // keeps the run open across it, exactly as the same producer
            // setting the marker in a separate text object (no kern, no
            // synthetic space) already does. ~keep
            if span.text.chars().all(char::is_whitespace) {
                continue;
            }

            if !is_heading_like(span) {
                if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
                continue;
            }

            if current.is_empty() {
                current.push(idx);
                continue;
            }

            let last_idx = *current.last().unwrap();
            let last = &spans[last_idx];

            let size_ok = (span.font_size - last.font_size).abs() <= font_eps;
            let bold_ok = span.font_weight.is_bold() == last.font_weight.is_bold();

            // Same-line: top within 1 pt of last's top — fold without
            // applying indent/leading checks (both spans belong to the
            // SAME wrapped-heading line, e.g. two bold Tj segments). ~keep
            let same_line = (span.bbox.top() - last.bbox.top()).abs() <= 1.0;

            if size_ok && bold_ok && same_line {
                current.push(idx);
                continue;
            }

            // Different line: enforce indent (4) + leading (5).
            // line_height = max of the two spans' bbox heights, plus a
            // floor of font_size to handle ascender-only / descender-only
            // glyphs with collapsed bboxes. ~keep
            let line_h = last.bbox.height.max(span.bbox.height).max(last.font_size).max(1.0);
            let leading_tolerance = line_h * 1.5;

            // PDF coords: y grows up, so the wrapped line sits at a
            // SMALLER bbox.top than the previous line. The gap between
            // last's bottom and span's top should fit inside the leading
            // tolerance. ~keep
            let last_bottom = last.bbox.top();
            // ~keep
            let span_top = span.bbox.top();
            let vertical_gap = (last_bottom - span_top).abs();

            let indent_ok = span.bbox.left() >= last.bbox.left() - indent_tolerance
                && span.bbox.left() <= last.bbox.left() + indent_tolerance;
            let leading_ok = vertical_gap <= leading_tolerance;

            if size_ok && bold_ok && indent_ok && leading_ok {
                current.push(idx);
            } else {
                runs.push(std::mem::take(&mut current));
                current.push(idx);
            }
        }
        if !current.is_empty() {
            runs.push(current);
        }

        // A run becomes a HeadingRun only when it spans ≥ 2 distinct
        // lines. Single-line bold spans (inline emphasis, lone short
        // headings) don't need locking — XY-cut handles them correctly
        // already, and locking them would be a no-op for the splitter
        // but adds overhead. ~keep
        runs.into_iter()
            .filter_map(|span_indices| {
                if span_indices.len() < 2 {
                    return None;
                }
                let mut distinct_lines = std::collections::BTreeSet::new();
                for &i in &span_indices {
                    distinct_lines.insert(spans[i].bbox.top().round() as i32);
                }
                if distinct_lines.len() < 2 {
                    return None;
                }
                Some(HeadingRun {
                    combined_bbox: union_bboxes(spans, &span_indices),
                    span_indices,
                })
            })
            .collect()
    }

    /// Build a synthetic span list where each detected `HeadingRun`
    /// collapses to ONE wide synthetic span carrying the union bbox.
    /// Non-heading spans pass through unchanged.
    ///
    /// Returns:
    /// - `synthetic`: the input to `partition_indexed`.
    /// - `synthetic_origin[k]`: indices of ORIGINAL spans backing
    ///   synthetic span `k`. Length 1 for pass-throughs, ≥ 2 for
    ///   heading-run placeholders. Used by `expand_blocks` to project
    ///   partition output back into original-span space.
    fn synthesize_for_partition(&self, spans: &[TextSpan], runs: &[HeadingRun]) -> (Vec<TextSpan>, Vec<Vec<usize>>) {
        let mut in_run: Vec<Option<usize>> = vec![None; spans.len()];
        for (r_idx, run) in runs.iter().enumerate() {
            for &i in &run.span_indices {
                in_run[i] = Some(r_idx);
            }
        }

        let mut synthetic: Vec<TextSpan> = Vec::with_capacity(spans.len());
        let mut origins: Vec<Vec<usize>> = Vec::with_capacity(spans.len());
        let mut emitted_run = vec![false; runs.len()];

        for (i, span) in spans.iter().enumerate() {
            match in_run[i] {
                None => {
                    synthetic.push(span.clone());
                    origins.push(vec![i]);
                }
                Some(r_idx) if !emitted_run[r_idx] => {
                    let run = &runs[r_idx];
                    let mut placeholder = span.clone();
                    placeholder.bbox = run.combined_bbox;
                    // Concatenate the run's text with single spaces so
                    // is_single_column_region's core-width estimate is
                    // proportional to the actual heading length, not the
                    // single first-line fragment. ~keep
                    let mut combined_text = String::new();
                    for (k, &si) in run.span_indices.iter().enumerate() {
                        if k > 0 {
                            combined_text.push(' ');
                        }
                        combined_text.push_str(&spans[si].text);
                    }
                    placeholder.text = combined_text;
                    synthetic.push(placeholder);
                    origins.push(run.span_indices.clone());
                    emitted_run[r_idx] = true;
                }
                Some(_) => {}
            }
        }

        (synthetic, origins)
    }

    /// Project partition groups from synthetic-span space back into
    /// original-span space, expanding each heading-run placeholder into
    /// its constituent original spans (in their original ordering).
    fn expand_blocks(
        &self,
        synth_groups: Vec<Vec<usize>>,
        original: &[TextSpan],
        synthetic_origin: &[Vec<usize>],
    ) -> Vec<Vec<TextSpan>> {
        synth_groups
            .into_iter()
            .map(|group| {
                let mut out = Vec::with_capacity(group.len());
                for synth_idx in group {
                    for &orig_idx in &synthetic_origin[synth_idx] {
                        out.push(original[orig_idx].clone());
                    }
                }
                out
            })
            .collect()
    }

    /// Index-based recursive partitioning — returns groups of indices into the input span slice.
    ///
    /// Avoids cloning TextSpan at every recursive split level. Spans are only
    /// read through shared reference; indices are partitioned instead.
    fn partition_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Vec<Vec<usize>> {
        self.partition_indexed_depth(all_spans, indices, 0)
    }

    /// Depth-bounded recursive partition. `find_vertical_split_indexed` permits
    /// singleton peels, so without a cap a page with many distinct-Y
    /// header/footer strips can recurse O(n) deep (O(n² log n) work);
    /// `MAX_PARTITION_DEPTH` bounds it.
    fn partition_indexed_depth(&self, all_spans: &[TextSpan], indices: &[usize], depth: u32) -> Vec<Vec<usize>> {
        if indices.is_empty() {
            return Vec::new();
        }

        // Base case: small region, don't split further via the recursive
        // partitioner. Below `min_spans_for_split`, the statistical
        // prose/table classifiers (`classify_region_kind`,
        // `detect_two_column_prose`, `detect_narrow_gutter_prose`) all have
        // their own internal minimum-span floors (6/8/24) far above this
        // one and unconditionally decline to classify — so a flat
        // "impose Y-then-X row-major order" was applied even to a genuine
        // sparse 2-column page (a 2-column, 2-row prose page —
        // 4 spans — read back row-major/interleaved instead of
        // column-major).
        //
        // A geometric gutter check alone can't distinguish "sparse 2-column
        // prose" from "a 2x2 row-major table" at this scale either — both
        // produce an identical clean-gutter signature, and the table-row
        // guard inside `find_horizontal_split_indexed` needs >=3 rows to
        // reach confidence, so it's a no-op at 2 rows. Rather than commit
        // to a (left, right) column grouping we can't justify being
        // correct, fall back to the page's own content-stream emission
        // order when a clean gutter exists at all (PDFium parity, per the
        // reporter's own cross-tool probe: PDFium performs no prose/table
        // decision here either, it just follows stream order). This
        // relies on the empirical tendency of table generators to emit
        // cells row-major and column-generators to emit column-major, in
        // the absence of any other geometric signal being decidable at
        // this scale. Falls back to the flat Y-then-X sort when no clean
        // gutter is found at all (ordinary short single-column snippets). ~keep
        if indices.len() < self.min_spans_for_split {
            if self.find_horizontal_split_indexed(all_spans, indices).is_some() {
                let mut stream_order: Vec<usize> = indices.to_vec();
                stream_order.sort_by_key(|&i| all_spans[i].sequence);
                return vec![stream_order];
            }
            return vec![self.sort_indices(all_spans, indices)];
        }

        if depth >= MAX_PARTITION_DEPTH {
            return vec![self.sort_indices(all_spans, indices)];
        }

        // Two-column-prose probe BEFORE the
        // single-column short-circuit. Tight gutters (~10-15pt) that
        // sit below `min_valley_width` defeat the standard projection-
        // valley detector, and the wide+dense heuristic inside
        // `is_single_column_region` mis-classifies the body as one
        // column because each line's bbox spans the narrow gutter.
        // The probe positively identifies the 2-column-prose shape
        // (gutter-radius left-edge clusters + ≥6 narrow lines +
        // classify_region_kind == Prose) and only fires when ALL of
        // those signals agree. Critically, the Prose gate prevents
        // the false positive that reverted earlier attempts on a
        // 2-column sub-region of the google_doc population table
        // (mean_chars < 8 → Table → bail).
        //
        // **Band-separation first**: when the probe would fire AND a
        // clean vertical band-separation (top header / body / bottom
        // footer) is available, peel the band off BEFORE the column
        // cut. Without this step, full-width header / footer rows
        // get absorbed into one of the two column halves and end up
        // mid-page in reading order — the failure mode on the
        // 1256-page French Bible where the chapter-header
        // band and page-number footer were full-width and span the
        // gutter. The signal for "band": a vertical split whose
        // smaller side has ≤ 25 % of the region's spans (a tight
        // band relative to the body it sits next to).
        // Two-column-prose detector based on line-start clustering.
        // When it fires, peel any wide Y-band first (title / authors
        // / abstract / footer often span the gutter) before the
        // column cut, so they don't get fragmented across columns.
        // Each peeled band is re-classified inside the recursive
        // call.
        //
        // Classify once and pass to both prose detectors below; each gated on
        // `classify_region_kind == Prose` and re-ran the same line clustering. ~keep
        let region_kind = self.classify_region_kind(all_spans, indices);
        if let Some(gutter_x) = self.detect_two_column_prose(all_spans, indices, region_kind) {
            if let Some((above, below)) = self.find_vertical_split_indexed(all_spans, indices) {
                tracing::trace!(
                    above = above.len(),
                    below = below.len(),
                    "peeling Y-band before column cut"
                );
                let mut result = self.partition_indexed_depth(all_spans, &above, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &below, depth + 1));
                return result;
            }
            let (left, right): (Vec<usize>, Vec<usize>) = indices
                .iter()
                .copied()
                .partition(|&i| all_spans[i].bbox.left() < gutter_x);
            if !left.is_empty() && !right.is_empty() {
                tracing::trace!(
                    gutter_x,
                    left = left.len(),
                    right = right.len(),
                    "two-column-prose detected"
                );
                let mut result = self.partition_indexed_depth(all_spans, &left, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &right, depth + 1));
                return result;
            }
        }

        // Narrow-gutter prose detector — second pass for layouts
        // where the line-start cluster shape is masked by outlier
        // singletons (title / caption / equation rows scattering
        // extra clusters that block the primary detector). Cuts
        // directly at the gap-cluster centre WITHOUT peeling a
        // Y-band first: for these pages `find_vertical_split`
        // tends to fire on mid-body paragraph gaps and bisect
        // the body across the peel — both halves then lose
        // enough gutter signal that the column cut never reaches
        // them on recursion. ~keep
        if let Some(gutter_x) = self.detect_narrow_gutter_prose(all_spans, indices, region_kind) {
            let (left, right): (Vec<usize>, Vec<usize>) = indices
                .iter()
                .copied()
                .partition(|&i| all_spans[i].bbox.left() < gutter_x);
            if !left.is_empty() && !right.is_empty() {
                tracing::trace!(
                    gutter_x,
                    left = left.len(),
                    right = right.len(),
                    "narrow-gutter prose detected"
                );
                let mut result = self.partition_indexed_depth(all_spans, &left, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &right, depth + 1));
                return result;
            }
        }

        // Detect single-column body text up-front and skip all spatial
        // splits. Real body text has density dips (indented code, short
        // last-lines, paragraph breaks) that would otherwise trigger
        // spurious horizontal (column) or vertical (row) splits,
        // scrambling reading order. The subsequent sort-by-Y already
        // handles row order within a column. ~keep
        if self.is_single_column_region(all_spans, indices) {
            return vec![self.sort_indices(all_spans, indices)];
        }

        let split_h = |s: &Self, sp: &[TextSpan], idx: &[usize]| s.find_horizontal_split_indexed(sp, idx);
        let split_v = |s: &Self, sp: &[TextSpan], idx: &[usize]| s.find_vertical_split_indexed(sp, idx);

        let first_split = if self.prefer_horizontal { split_h } else { split_v };
        let second_split = if self.prefer_horizontal { split_v } else { split_h };

        if let Some((a, b)) = first_split(self, all_spans, indices) {
            let mut result = self.partition_indexed_depth(all_spans, &a, depth + 1);
            result.extend(self.partition_indexed_depth(all_spans, &b, depth + 1));
            return result;
        }

        if let Some((a, b)) = second_split(self, all_spans, indices) {
            let mut result = self.partition_indexed_depth(all_spans, &a, depth + 1);
            result.extend(self.partition_indexed_depth(all_spans, &b, depth + 1));
            return result;
        }

        vec![self.sort_indices(all_spans, indices)]
    }

    /// Classifier verdict for a region — used to gate the tight-gutter
    /// column-split path so the same XY-cut recursion no longer
    /// corrupts table cells (the lesson).
    ///
    /// See the inline post-mortem at lines 73–101: two prior attempts at
    /// the multi-column-prose fix were reverted by the 70-PDF sweep when
    /// they accidentally fired on a 2-column sub-region of a real table
    /// and reordered digits. The fix has to *positively identify prose*
    /// before allowing the tight cut — not merely *fail to identify
    /// table*. This classifier is that positive identification.
    fn classify_region_kind(&self, all_spans: &[TextSpan], indices: &[usize]) -> RegionKind {
        if indices.len() < 6 {
            return RegionKind::Mixed;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width <= 10.0 {
            return RegionKind::Mixed;
        }

        let mut lines: std::collections::BTreeMap<i32, (f32, f32, usize)> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let nonws_chars = s.text.chars().filter(|c| !c.is_whitespace()).count();
            let entry = lines.entry(y_key).or_insert((f32::MAX, f32::MIN, 0));
            entry.0 = entry.0.min(s.bbox.left());
            entry.1 = entry.1.max(s.bbox.right());
            entry.2 += nonws_chars;
        }

        let line_count = lines.len();
        if line_count < 6 {
            // Too few lines to be a substantial prose body. Headings,
            // captions, single paragraphs all land here — leave them to
            // the default XY-cut behaviour. ~keep
            return RegionKind::Mixed;
        }

        // Per-line statistics: average char count and the count of
        // "narrow" lines whose extent < 0.6 × region_width (a column-half
        // line) and "wide" lines whose extent ≥ 0.6 × region_width (a
        // body-text or table-row line). Table cells are narrow; tables
        // have many such narrow lines but with very short content. ~keep
        let mut total_chars = 0usize;
        let mut narrow_lines = 0usize;
        let mut wide_lines = 0usize;
        for (left, right, chars) in lines.values() {
            total_chars += chars;
            let extent = (*right - *left).max(0.0);
            if extent < region_width * 0.6 {
                narrow_lines += 1;
            } else {
                wide_lines += 1;
            }
        }
        let mean_chars = total_chars as f32 / line_count as f32;

        // PROSE: tall stack of wide lines OR tall stack of half-column
        // lines with substantial content per line.
        //   - mean_chars > 20: real prose, not table cells
        //   - line_count ≥ 6: substantial column
        //   - either:
        //     * majority of lines are wide (single-column body), OR
        //     * majority of lines are narrow with mean_chars > 20
        //       (two half-column lines with prose content) ~keep
        let mostly_wide = wide_lines * 2 > line_count;
        let mostly_narrow = narrow_lines * 2 > line_count;
        if mean_chars > 20.0 && (mostly_wide || mostly_narrow) {
            return RegionKind::Prose;
        }

        // SHORT-LINE PROSE (short-verse two-column bodies): the
        // `mean_chars > 20` guard above deliberately rejected short-verse
        // two-column bodies (Bible / lexicon editions — a verse fragment
        // per column-line is often < 20 non-whitespace chars) along with
        // short-cell tables. The guard was doing two jobs at once. Here we
        // re-admit ONLY the short-line case that carries a *strong central
        // gutter corridor* a short-cell table cannot fake: a single
        // persistent vertical gutter near the region centre, present on a
        // high fraction of lines, with balanced left/right char mass and
        // ≤ 2 left-edge clusters. A label+data table fails this on
        // concentration/coverage (its gaps scatter across cell
        // boundaries), centre (the dominant gap sits off-centre),
        // char-balance (the label column is tiny), or left-edge clusters
        // (≥ 3 columns). The long-line accept path above is byte-unchanged. ~keep
        if mean_chars <= 20.0 && self.short_line_central_corridor_prose(all_spans, indices, x_min, region_width) {
            return RegionKind::Prose;
        }

        // TABLE: lots of narrow lines, short content per line (mean_chars
        // < 8). The google_doc population table —
        // the canonical regression that reverted attempts 1 & 2 — sits
        // squarely here (digit-only cells, ≤ 7 chars each). ~keep
        if mean_chars < 8.0 {
            return RegionKind::Table;
        }

        // Anything in between (e.g. captions with headings, mixed
        // figure-and-text bands) → don't risk the tight cut. ~keep
        RegionKind::Mixed
    }

    /// Short-line two-column-prose admission.
    ///
    /// Called from `classify_region_kind` ONLY for the short-line case
    /// (`mean_chars <= 20`) that the long-line prose guard rejects. A
    /// short-verse two-column body (verse-per-line bibles/lexicons) has
    /// short lines yet a strong, table-independent central gutter; a
    /// short-cell numeric table has short lines and NO such corridor.
    ///
    /// Returns `true` only when ALL of the following hold — each one a
    /// length-independent discriminator a short-cell label+data table
    /// cannot satisfy:
    ///   - a single persistent vertical gutter exists: per-line largest
    ///     within-line gap clusters at one X (10 pt radius) covering
    ///     **≥ 70 %** of gap-bearing lines (concentration) and present on
    ///     **≥ 60 %** of all lines (coverage) — a table's dominant gap
    ///     scatters across cell boundaries and appears on a minority of
    ///     rows;
    ///   - that gutter sits near the region centre: offset ∈
    ///     **[0.30, 0.70]·region_width** — a label+data table's dominant
    ///     gap sits off-centre;
    ///   - **left/right char balance:** non-whitespace char mass on each
    ///     side of the gutter is **≥ 35 %** of the total — a label column
    ///     is lopsided (one side is tiny numeric labels);
    ///   - **≤ 2 left-edge clusters** left of the gutter (30 pt radius) —
    ///     a real two-column body starts each column at one X; an
    ///     N-column table has ≥ 3 left-edge clusters (the fix-534
    ///     `left_edge_clusters >= 3 → Mixed` rule).
    fn short_line_central_corridor_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        x_min: f32,
        region_width: f32,
    ) -> bool {
        if region_width <= 0.0 {
            return false;
        }

        // Re-cluster spans into lines, keeping PER-SPAN (left, right, chars)
        // so we can find the within-line gutter gap and split char mass. ~keep
        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32, usize)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let nonws = s.text.chars().filter(|c| !c.is_whitespace()).count();
            lines
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right(), nonws));
        }
        let total_lines = lines.len();
        if total_lines == 0 {
            return false;
        }

        // Per-line: largest within-line gap and its midpoint X. A gap of
        // ≥ 6 pt suppresses ordinary 2–5 pt word spacing. ~keep
        const MIN_GAP_PT: f32 = 6.0;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            if line_spans.len() < 2 {
                continue;
            }
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut largest_mid = 0.0_f32;
            for w in sorted.windows(2) {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    largest_mid = (w[0].1 + w[1].0) * 0.5;
                }
            }
            if largest_gap >= MIN_GAP_PT {
                gap_positions.push(largest_mid);
            }
        }
        if gap_positions.is_empty() {
            return false;
        }

        const CLUSTER_RADIUS_PT: f32 = 10.0;
        let mut sorted_gaps = gap_positions.clone();
        sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
        let mut best_size = 0usize;
        let mut best_center = 0.0_f32;
        for &pivot in &sorted_gaps {
            let lo = pivot - CLUSTER_RADIUS_PT;
            let hi = pivot + CLUSTER_RADIUS_PT;
            let mut count = 0usize;
            let mut sum = 0.0_f32;
            for &g in &sorted_gaps {
                if g >= lo && g <= hi {
                    count += 1;
                    sum += g;
                }
            }
            if count > best_size {
                best_size = count;
                best_center = sum / count as f32;
            }
        }
        if best_size == 0 {
            return false;
        }

        // Concentration ≥ 70 % of gap-bearing lines at one X. ~keep
        if best_size * 10 < gap_positions.len() * 7 {
            return false;
        }
        // Coverage ≥ 60 % of ALL lines carry the corridor. ~keep
        if best_size * 10 < total_lines * 6 {
            return false;
        }
        let gutter_offset = best_center - x_min;
        if gutter_offset < region_width * 0.30 || gutter_offset > region_width * 0.70 {
            return false;
        }

        // Left/right non-whitespace char balance about the corridor:
        // each side ≥ 35 % of total. A label-column table is lopsided. ~keep
        let mut left_chars = 0usize;
        let mut right_chars = 0usize;
        for line_spans in lines.values() {
            for &(l, r, chars) in line_spans {
                let mid = (l + r) * 0.5;
                if mid < best_center {
                    left_chars += chars;
                } else {
                    right_chars += chars;
                }
            }
        }
        let total_chars = left_chars + right_chars;
        if total_chars == 0 {
            return false;
        }
        if (left_chars as f32) < total_chars as f32 * 0.35 || (right_chars as f32) < total_chars as f32 * 0.35 {
            return false;
        }

        // ≤ 2 left-edge clusters left of the corridor (30 pt radius). A
        // real two-column body starts its left column at one X (one
        // cluster, maybe two counting a paragraph indent); an N-column
        // table left of the corridor has several cell-start X's → ≥ 3
        // clusters. Cluster EVERY span left-edge that lies left of the
        // corridor (not just each line's minimum) so multi-column cell
        // starts are not collapsed into one cluster. ~keep
        const LEFT_CLUSTER_RADIUS_PT: f32 = 30.0;
        let mut clusters: Vec<(f32, usize)> = Vec::new();
        for line_spans in lines.values() {
            for &(l, _, _) in line_spans {
                if l >= best_center {
                    continue;
                }
                if let Some(c) = clusters
                    .iter_mut()
                    .find(|(c, _)| (*c - l).abs() <= LEFT_CLUSTER_RADIUS_PT)
                {
                    let count = c.1 as f32;
                    c.0 = (c.0 * count + l) / (count + 1.0);
                    c.1 += 1;
                } else {
                    clusters.push((l, 1));
                }
            }
        }
        // Drop singleton/noise clusters (< 2 lines) before counting, so a
        // lone outlier left-edge doesn't inflate the count. ~keep
        let dominant_left_clusters = clusters.iter().filter(|(_, n)| *n >= 2).count();
        if dominant_left_clusters >= 3 {
            return false;
        }

        true
    }

    /// Two-column-prose probe — does this region look like two
    /// side-by-side columns of prose with a tight gutter (~10-15pt)?
    ///
    /// Called from `is_single_column_region` when the wide+dense
    /// heuristic would otherwise short-circuit the region as
    /// single-column. Distinguishing signal: most lines fit inside
    /// **one** half of the region width (column-half lines), and the
    /// left edges cluster into exactly **two** groups separated by
    /// approximately half the region width.
    ///
    /// Gated on `classify_region_kind == Prose` so the same machinery
    /// doesn't fire on a 2-column sub-region of a table (an earlier
    /// failure mode).
    ///
    /// Returns `Some(gutter_x)` when a 2-column prose layout is
    /// detected — the caller treats that as a non-single-column verdict
    /// and lets `find_horizontal_split_indexed` cut at the gutter.
    fn detect_two_column_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        region_kind: RegionKind,
    ) -> Option<f32> {
        if indices.len() < 8 {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width < 200.0 {
            // Real two-column bodies span at least ~200pt (the
            // narrowest two-column layout in the corpus is ~250pt for a
            // letter-page body inside ~250pt margins). ~keep
            return None;
        }

        // Cluster spans into lines by rounded Y. Keep PER-SPAN
        // (left, right) data so we can detect within-line gaps —
        // the canonical multi-column interleave puts
        // a left-col span (left=82) and a right-col span (left=312)
        // on the same Y baseline. The whole-line bbox.right -
        // bbox.left = 358 pt looks "wide" (358 > 0.6 × 500 = 300)
        // even though each side is a narrow column half. ~keep
        let mut lines_spans: std::collections::BTreeMap<i32, Vec<(f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            lines_spans
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right()));
        }
        if lines_spans.len() < 6 {
            return None;
        }

        // For each line, find the largest gap between adjacent spans.
        // A line is treated as multiple "half-lines" if a gap ≥ 10 pt
        // splits it; each side of the gap contributes its leftmost-x
        // to `narrow_lefts`. This is the lesson: the row-by-
        // row interleave shape spans the gutter as bbox
        // but has a clear gap within each line. ~keep
        let narrow_threshold = region_width * 0.6;
        let intra_line_gap_threshold = 10.0_f32;
        let mut narrow_lefts: Vec<f32> = Vec::new();
        // Count "narrow" lines for the majority check — a line with
        // a within-line gap contributes 1 to this count regardless of
        // how many half-lines it produces, so the majority threshold
        // stays comparable to single-column reasoning. ~keep
        let mut narrow_line_count = 0usize;
        for line_spans in lines_spans.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut split_idx: Option<usize> = None;
            for (i, w) in sorted.windows(2).enumerate() {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    split_idx = Some(i);
                }
            }
            let line_left = sorted.first().map(|(l, _)| *l).unwrap_or(0.0);
            let line_right = sorted.last().map(|(_, r)| *r).unwrap_or(0.0);
            let line_extent = (line_right - line_left).max(0.0);

            if let Some(si) = split_idx
                && largest_gap >= intra_line_gap_threshold
            {
                narrow_lefts.push(line_left);
                if let Some(&(right_side_left, _)) = sorted.get(si + 1) {
                    narrow_lefts.push(right_side_left);
                }
                narrow_line_count += 1;
                continue;
            }

            if line_extent < narrow_threshold {
                narrow_lefts.push(line_left);
                narrow_line_count += 1;
            }
        }
        // Majority of lines must be narrow — otherwise this isn't a
        // 2-column body, it's a single-column body with a few short
        // last-lines. ~keep
        if narrow_line_count * 2 < lines_spans.len() {
            return None;
        }

        // Cluster the narrow left-edges. Two clusters separated by
        // approximately half the region width = 2-column prose. ~keep
        let cluster_radius = 30.0_f32;
        let mut clusters: Vec<(f32, usize)> = Vec::new();
        for &x in &narrow_lefts {
            if let Some(c) = clusters.iter_mut().find(|(c, _)| (*c - x).abs() <= cluster_radius) {
                let count = c.1 as f32;
                c.0 = (c.0 * count + x) / (count + 1.0);
                c.1 += 1;
            } else {
                clusters.push((x, 1));
            }
        }

        // Want exactly 2 substantial clusters separated by ~half-width.
        // ≥ 3 clusters = either a table or a band-mixed region — bail. ~keep
        if clusters.len() != 2 {
            return None;
        }
        clusters.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
        let (c1_x, c1_n) = clusters[0];
        let (c2_x, c2_n) = clusters[1];

        // Each cluster needs substantial coverage — ≥ 3 lines, or 20 %
        // of the line count, whichever is larger. Reject lopsided
        // shapes (header + body-paragraph). ~keep
        let min_cluster = 3usize.max(narrow_lefts.len() / 5);
        if c1_n < min_cluster || c2_n < min_cluster {
            return None;
        }

        // Gap between cluster centres ≥ 30 % of region width (the
        // gutter + right-column left-margin). For a tight gutter of
        // ~12pt with two ~250pt columns the gap is ~250pt out of 512pt
        // → ~49 %, well above the floor. ~keep
        let gap = c2_x - c1_x;
        if gap < region_width * 0.30 {
            return None;
        }

        // Positive identification of prose — required by the
        // classifier to avoid the google_doc 2-col table
        // sub-region false positive. ~keep
        if region_kind != RegionKind::Prose {
            return None;
        }

        // Gutter midpoint as the cut. The cluster centres are the left
        // edges of the two columns; the gutter sits between the right
        // edge of column 1 and the left edge of column 2. We don't
        // track right edges per cluster, so approximate the gutter
        // centre as halfway between the two cluster centres — that's
        // close enough; the actual partition uses `bbox.left()` per
        // span so individual spans land cleanly on either side. ~keep
        let gutter_x = (c1_x + c2_x) * 0.5;
        Some(gutter_x)
    }

    /// Second-pass 2-column-prose detector for the narrow-gutter case
    /// that `detect_two_column_prose` (the line-start-cluster detector)
    /// misses.
    ///
    /// Two-column papers that emit body text at character-cluster
    /// granularity (each glyph its own span) confuse the line-start
    /// detector: titles, captions, and equation labels contribute
    /// outlier singleton clusters in addition to the two body
    /// columns, so the `clusters.len() != 2` gate rejects. Their
    /// gutters are also often narrower than `min_valley_width` so
    /// the primary projection-valley path in
    /// `find_horizontal_split_indexed` rejects as well.
    ///
    /// Distinguishing signal that works regardless of outlier rows:
    /// the **largest within-line gap** on each body line lives at
    /// roughly the same X coordinate (the gutter) across a strong
    /// majority of lines. Cluster those gap positions; if one cluster
    /// covers ≥ 60 % of the body lines AND the region classifies as
    /// `Prose`, the page is two-column prose and the cluster centre
    /// is the gutter X.
    ///
    /// Returns the gutter X coordinate (an actual gap position, not
    /// a midpoint estimate) when the pattern is detected.
    ///
    /// The Prose-classifier gate keeps tables out: table rows have
    /// their largest gap at variable X across rows (different cell
    /// widths), so the gap-position cluster never dominates.
    fn detect_narrow_gutter_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        region_kind: RegionKind,
    ) -> Option<f32> {
        if indices.len() < 24 {
            return None;
        }
        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width < 200.0 {
            return None;
        }

        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            lines.entry(y_key).or_default().push((s.bbox.left(), s.bbox.right()));
        }
        if lines.len() < 12 {
            return None;
        }

        // For each line, find the largest within-line gap (≥ 6 pt
        // suppresses ordinary word-spacing of 2–5 pt). Record the gap's
        // midpoint X. ~keep
        const MIN_GAP_PT: f32 = 6.0;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            if line_spans.len() < 2 {
                continue;
            }
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut largest_mid = 0.0_f32;
            for w in sorted.windows(2) {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    largest_mid = (w[0].1 + w[1].0) * 0.5;
                }
            }
            if largest_gap >= MIN_GAP_PT {
                gap_positions.push(largest_mid);
            }
        }

        // Need at least 12 gap-bearing lines to cluster — fewer is
        // statistical noise. ~keep
        if gap_positions.len() < 12 {
            return None;
        }

        // Cluster the gap positions with a 10 pt radius (tight; the
        // gutter is at one specific X with minor line-to-line drift).
        // Sliding-window two-pointer scan over the sorted positions —
        // both `left` and `right` only advance forward, so total
        // work is O(n) instead of the previous O(n²) pivot scan
        // (thesis-style PDFs with hundreds of gap-bearing rows pay
        // visibly in that nested loop). ~keep
        const CLUSTER_RADIUS_PT: f32 = 10.0;
        let mut sorted_gaps = gap_positions.clone();
        sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
        // Prefix sums let us read window-sum in O(1) given (left, right). ~keep
        let mut prefix: Vec<f32> = Vec::with_capacity(sorted_gaps.len() + 1);
        prefix.push(0.0);
        for &x in &sorted_gaps {
            prefix.push(prefix.last().unwrap() + x);
        }
        let mut best_size = 0usize;
        let mut best_center = 0.0_f32;
        let mut left = 0usize;
        let mut right = 0usize;
        for &pivot in &sorted_gaps {
            while left < sorted_gaps.len() && sorted_gaps[left] < pivot - CLUSTER_RADIUS_PT {
                left += 1;
            }
            while right < sorted_gaps.len() && sorted_gaps[right] <= pivot + CLUSTER_RADIUS_PT {
                right += 1;
            }
            let count = right - left;
            let sum = prefix[right] - prefix[left];
            if count > best_size {
                best_size = count;
                best_center = sum / count as f32;
            }
        }

        // Concentration: ≥ 70 % of gap-bearing lines cluster at the
        // same X. Distinguishes 2-col prose (one gutter) from
        // tables (gaps at several cell boundaries, lower
        // concentration). ~keep
        if best_size * 10 < gap_positions.len() * 7 {
            return None;
        }
        if best_size < 12 {
            return None;
        }
        if best_size * 5 < lines.len() {
            return None;
        }

        let gutter_offset = best_center - x_min;
        if gutter_offset < region_width * 0.2 || gutter_offset > region_width * 0.8 {
            return None;
        }

        // Prose gate — same safety as `detect_two_column_prose`.
        // Tables with narrow cell gaps fail the classifier
        // (`mean_chars < 8` → `Table`), preventing the gap-cluster
        // signal from misfiring on tabular content. Short-verse
        // two-column bodies now also pass this gate: although
        // their `mean_chars <= 20`, `classify_region_kind`'s short-line
        // central-corridor admission arm returns `Prose` for them, so a
        // routed short-verse body is cut here rather than re-collapsed.
        // ~keep
        if region_kind != RegionKind::Prose {
            return None;
        }

        Some(best_center)
    }

    /// Heuristic: does the region look like a single column of body text?
    ///
    /// Called **before** horizontal split attempts. When true, the region
    /// is returned as a single sorted group, bypassing both horizontal
    /// (column) and vertical (row) splits. This prevents XY-Cut from
    /// fragmenting body text at density dips caused by indentation or
    /// short last-lines.
    ///
    /// Detection: cluster spans into lines by rounded top-Y, then count
    /// lines that are both **wide** (extent ≥ 60% region width) and
    /// **dense** (covered ratio ≥ 80%). Body-text lines satisfy both.
    /// Aligned multi-column rows look "wide" because their extent spans
    /// the gutter, but fail the density check because the gutter is empty.
    fn is_single_column_region(&self, all_spans: &[TextSpan], indices: &[usize]) -> bool {
        if indices.len() < 3 {
            return false;
        }
        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width <= 10.0 {
            return true;
        }

        // Store both bbox.right and core_right for each span. bbox.right
        // can be over-estimated by extractors (trailing whitespace,
        // stretched advance widths) which makes multi-column lines look
        // like one wide continuous run; core_right (char_count × em) is
        // a conservative fallback used ONLY when adjacent bbox edges
        // overlap (a signal of bbox inflation).
        // ~keep
        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let char_count = s.text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f32;
            let approx_char_width = (s.font_size * 0.45).max(2.5);
            let core_right = s.bbox.left() + char_count * approx_char_width;
            lines
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right(), core_right));
        }
        if lines.len() < 3 {
            return false;
        }

        // A real column gutter recurs at roughly the SAME X position
        // across multiple lines. Sparse title-page layouts (Title /
        // Subtitle / Byline) also have wide inter-word gaps, but their
        // gap positions are scattered — not a gutter. Collect all gap
        // positions (mid-gap X), then check whether a consistent cluster
        // of gap positions appears on ≥30% of lines.
        //
        // Gap uses bbox.right, but if adjacent bboxes OVERLAP (classic
        // signature of extractor-inflated bbox widths), re-check with
        // conservative core_right estimates so column detection is not
        // defeated by trailing whitespace inflation. ~keep
        let max_gap = self.min_valley_width;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            for w in sorted.windows(2) {
                let bbox_gap = w[1].0 - w[0].1;
                let (effective_gap, gap_end_left) = if bbox_gap < 0.0 {
                    (w[1].0 - w[0].2, w[0].2)
                } else {
                    (bbox_gap, w[0].1)
                };
                if effective_gap >= max_gap {
                    gap_positions.push((gap_end_left + w[1].0) * 0.5);
                }
            }
        }
        // Centered-block guard: a CENTERED title/subtitle/
        // byline block (each line horizontally centered, varying widths)
        // produces accidental gap clusters that look like a column
        // gutter — but it is NOT columnar, and treating it as columns
        // scrambles reading order ("Quarterly Inventory Review" centered
        // title read as 3 columns → "Quarterly" / "Spring" / ... ).
        //
        // The distinguishing signal: a REAL multi-column layout has the
        // left column starting at a consistent left edge across rows
        // (low variance of per-line leftmost x). Centered text has its
        // leftmost x scattered (each line centered with a different
        // width). Compute the spread of per-line leftmost edges; if it
        // is large relative to the region width, the block is centered,
        // not columnar, so do NOT treat the gap cluster as a gutter.
        // Centered iff the per-line leftmost edges do NOT share a common
        // left margin. A left-aligned layout (single column OR real
        // multi-column) has most rows starting at the same x (the left
        // margin), so the largest cluster of leftmost edges covers a
        // majority of lines. Centered text has each line's leftmost edge
        // scattered (different per line), so no cluster dominates.
        //
        // Using a cluster fraction (not raw spread) is robust to rows
        // that only contain right-column content — those push the spread
        // up but do not change the fact that the left margin still
        // dominates the remaining rows. (Raw spread mis-classified the
        // two-column test where the last row held only a right cell.) ~keep
        let looks_centered = {
            let mins: Vec<f32> = lines
                .values()
                .map(|ls| ls.iter().map(|(l, _, _)| *l).fold(f32::MAX, f32::min))
                .collect();
            if mins.len() < 2 {
                false
            } else {
                let tol = 10.0_f32;
                // Largest count of leftmost-edges within ±tol of any single edge.
                // Sort once + binary-search the window instead of the O(k^2)
                // all-pairs scan; the max count is a multiset property so this is
                // identical to the pairwise version. ~keep
                let largest = {
                    let mut sorted = mins.clone();
                    sorted.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
                    sorted
                        .iter()
                        .map(|&a| {
                            let lo = sorted.partition_point(|&x| x < a - tol);
                            let hi = sorted.partition_point(|&x| x <= a + tol);
                            hi - lo
                        })
                        .max()
                        .unwrap_or(0)
                };
                (largest as f32) < (mins.len() as f32) * 0.5
            }
        };

        // A SMALL centered block (title / subtitle / byline — few lines,
        // scattered leftmost edges) is treated as a single column so its
        // lines stay in top-to-bottom order and a centered multi-word
        // title is not split into per-word "columns". Gated
        // to <= 6 lines so it only catches title-page-style blocks: a
        // real multi-column body has many lines and is never classified
        // centered here (its left column starts at a consistent margin,
        // giving a small leftmost-spread anyway). ~keep
        if looks_centered && lines.len() <= 6 {
            return true;
        }

        // Cluster gap positions: count, for each observed gap, how many
        // other gaps fall within ±20pt. If any cluster contains gaps
        // from ≥30% of lines, it's a genuine column gutter. ~keep
        if !gap_positions.is_empty() && !looks_centered {
            let cluster_radius = 20.0_f32;
            // Require ≥3 gap positions (or 20% of lines, whichever is
            // larger) clustered within ±20pt. 20% accommodates pages
            // where header/footer/title rows dilute the body-line count
            // but a real multi-column body still dominates. ~keep
            let min_cluster = (3usize).max(lines.len() / 5);
            // Sort once + binary-search each gap's ±radius window instead of the
            // O(k^2) all-pairs scan. Returns false iff some gap's window holds
            // >= min_cluster gaps — identical to the pairwise version. ~keep
            let mut sorted_gaps = gap_positions.clone();
            sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            for &pos in &sorted_gaps {
                let lo = sorted_gaps.partition_point(|&p| p < pos - cluster_radius);
                let hi = sorted_gaps.partition_point(|&p| p <= pos + cluster_radius);
                if hi - lo >= min_cluster {
                    return false;
                }
            }
        }

        let width_threshold = region_width * 0.6;
        let mut wide_dense_lines = 0usize;
        for line_spans in lines.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let extent_left = sorted.first().unwrap().0;
            let extent_right = sorted.iter().map(|(_, r, _)| *r).fold(f32::MIN, f32::max);
            let extent = extent_right - extent_left;
            if extent < width_threshold {
                continue;
            }
            // Use core_right (char-count estimate) rather than bbox.right
            // for coverage. bbox.right is inflated by tab characters and
            // trailing whitespace — tab-expanded table rows would otherwise
            // score 100% coverage and be misidentified as dense body text. ~keep
            let mut covered = 0.0f32;
            let mut last_end = f32::MIN;
            for &(l, _, cr) in &sorted {
                let effective_right = cr.min(extent_right);
                let start = l.max(last_end);
                if effective_right > start {
                    covered += effective_right - start;
                    last_end = effective_right;
                }
            }
            if covered >= extent * 0.8 {
                wide_dense_lines += 1;
            }
        }
        wide_dense_lines * 2 >= lines.len()
    }

    /// Find vertical line (X-axis) split using index-based partitioning.
    ///
    /// Rejects lopsided splits where one side contains fewer than ~10% of
    /// the region's spans — those come from single-column pages where
    /// indentation or stray content creates a spurious density dip at
    /// one edge of the projection, not from a real column boundary.
    fn find_horizontal_split_indexed(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
    ) -> Option<(Vec<usize>, Vec<usize>)> {
        let profile = self.horizontal_projection_indexed(all_spans, indices)?;

        let split_x = if let Some((vs, ve, vw)) = self.find_valley(&profile) {
            if vw < self.min_valley_width {
                return None;
            }
            profile.x_min + (vs + ve) as f32 / 2.0
        } else {
            self.find_split_between_peaks(&profile)?
        };

        // Reject splits where either resulting sub-column would be
        // narrower than ~60 pt (about 6 body-text characters at
        // 10 pt). Without this check, XY-cut recursion sub-splits
        // a single body column into sliver sub-blocks at internal
        // whitespace valleys (paragraph indentation, justified-line
        // trailing gaps, isolated short words), turning what should
        // be a clean column-major emit of a multi-column page into
        // a band-chunked stream. PDF spec §9.4.4 mentions "natural
        // reading order" but does not mandate a
        // minimum column width; this is a descriptive heuristic —
        // a real body column holds at least ~6 characters. ~keep
        const MIN_RESULT_WIDTH_PT: f32 = 60.0;
        let mut left_x_min = f32::MAX;
        let mut left_x_max = f32::MIN;
        let mut right_x_min = f32::MAX;
        let mut right_x_max = f32::MIN;
        for &i in indices {
            let l = all_spans[i].bbox.left();
            let r = all_spans[i].bbox.right();
            if l < split_x {
                left_x_min = left_x_min.min(l);
                left_x_max = left_x_max.max(r);
            } else {
                right_x_min = right_x_min.min(l);
                right_x_max = right_x_max.max(r);
            }
        }
        let left_w = left_x_max - left_x_min;
        let right_w = right_x_max - right_x_min;
        if left_w < MIN_RESULT_WIDTH_PT || right_w < MIN_RESULT_WIDTH_PT {
            return None;
        }

        // Partition by span LEFT EDGE (where the glyphs actually start),
        // not bbox.right() and not center. Extractor bboxes overreach to
        // the right (trailing whitespace / stretched advance widths), and
        // for wide single-column body spans the center can also drift
        // past the split. Left edge is anchored to the true glyph start
        // and reliably places each span into its actual column. ~keep
        let (left, right): (Vec<usize>, Vec<usize>) =
            indices.iter().partition(|&&i| all_spans[i].bbox.left() < split_x);

        if left.is_empty() || right.is_empty() {
            return None;
        }

        // Real column splits produce balanced partitions. A 95/5 split is
        // almost always from edge dips or stray content, not a column. ~keep
        let min_side = (indices.len() / 10).max(2);
        if left.len() < min_side || right.len() < min_side {
            return None;
        }

        // Table-row guard (PMC8025747). A genuine column gutter is a
        // vertical CORRIDOR: the left column's glyphs END before the gutter
        // and the right column's glyphs BEGIN after it, so the two sides are
        // X-disjoint. A data-table row, by contrast, starts at the left
        // margin but its cells run the FULL width of the region; partitioning
        // such rows by left edge throws the wide rows into `left` while the
        // right-hand cells (their own spans) land in `right`. Taking the cut
        // anyway slices the table's rows into shattered left/right cell groups
        // — the canonical PMC8025747 p2 failure (a prose column stacked above
        // a full-width data table), and the google_doc population-table hazard
        // the post-mortem at lines 73–101 records.
        //
        // Table-row SIGNATURE: SEVERAL left-side rows each span the ENTIRE
        // right column — their glyph content reaches past the right column's
        // far edge (`right_x_max`). A data table has MANY full-width rows (the
        // header and every data row run the whole region width), so when rows
        // are bucketed into `left` by their left edge, multiple of them blanket
        // the whole right column. By contrast:
        //   * a genuine left prose / reference column ENDS before the gutter,
        //     so its lines stop well short of `right_x_max` (never counted);
        //   * a single wide mis-split OCR line (alice) yields at most one or
        //     two straddling spans — never the recurring full-width pattern;
        //   * the single-column google_doc population table short-circuits at
        //     `is_single_column_region` and never reaches here.
        // Requiring ≥ 3 such rows is what isolates the real table from those
        // cases, so the guard is SUBTRACTIVE: it only ever REJECTS a column
        // cut that would shred a table (the recursion then falls back to a row
        // cut and reads the table row-major), never adds or reorders anything.
        //
        // `core_right` (left edge + non-whitespace-char count × ~0.5 em) is
        // used instead of `bbox.right` so trailing-whitespace / advance-width
        // bbox inflation on a real left column's last word is not mistaken for
        // a glyph crossing the gutter. `overlap_tol` (~ one body em) lets a
        // single straddling glyph slip past. ~keep
        let mut right_x_max = f32::MIN;
        let mut max_font = 0.0f32;
        for &i in &right {
            right_x_max = right_x_max.max(all_spans[i].bbox.right());
            max_font = max_font.max(all_spans[i].bbox.height.abs());
        }
        let overlap_tol = max_font.max(10.0);
        let full_width_left_rows = left
            .iter()
            .filter(|&&i| {
                let s = &all_spans[i];
                let nonws = s.text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f32;
                let approx_char_width = (s.font_size * 0.45).max(2.5);
                s.bbox.left() + nonws * approx_char_width >= right_x_max - overlap_tol
            })
            .count();
        if full_width_left_rows >= 3 {
            // ≥ 3 left rows each blanket the right column ⇒ this is a table-row
            // slice, not a column gutter. Don't take the column cut; the
            // recursion falls back to a row (horizontal) split and reads the
            // table row-major. ~keep
            return None;
        }

        Some((left, right))
    }

    /// Fallback column split: find the deepest trough between the two
    /// strongest density peaks. Used when the standard valley detection
    /// fails because narrow table-cell spans partially fill the gutter.
    ///
    /// Returns the split X coordinate (absolute, not relative to x_min) if
    /// a genuine trough exists — i.e., the minimum between the peaks is ≤
    /// 50% of the weaker peak density.
    fn find_split_between_peaks(&self, profile: &ProjectionProfile) -> Option<f32> {
        let density = &profile.density;
        let n = density.len();
        if n < 3 {
            return None;
        }

        // Smooth with a small box filter (window = min_valley_width) to
        // average out individual narrow peaks before finding mass centres. ~keep
        let smooth_window = (self.min_valley_width as usize).max(3);
        let half = smooth_window / 2;

        // Smooth into a reused thread-local buffer instead of a fresh `Vec` per
        // failed-valley node. Window-mean is unchanged. (Confirmed not a source
        // of the p.692 non-determinism: the buffer is cleared+refilled to exactly
        // `n` each call and never read out of range.) ~keep
        thread_local! {
            static SMOOTH_SCRATCH: std::cell::RefCell<Vec<f32>> =
                const { std::cell::RefCell::new(Vec::new()) };
        }
        SMOOTH_SCRATCH.with(|cell| {
            let mut smoothed = cell.borrow_mut();
            smoothed.clear();
            smoothed.extend((0..n).map(|i| {
                let s = i.saturating_sub(half);
                let e = (i + half + 1).min(n);
                let sum: f32 = density[s..e].iter().sum();
                sum / (e - s) as f32
            }));

            // Find the strongest peak in each half. Use `safe_float_cmp` for
            // NaN-safe total ordering — matches the comparator used elsewhere
            // in the reading-order code so `density` sentinel values can't
            // reach a `partial_cmp` that maps them to `Equal`. ~keep
            let mid = n / 2;
            let left_peak = (0..mid).max_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;
            let right_peak = (mid..n).max_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;

            if smoothed[left_peak] == 0.0 || smoothed[right_peak] == 0.0 {
                return None;
            }

            let search_start = left_peak.min(right_peak) + 1;
            let search_end = left_peak.max(right_peak);
            if search_start >= search_end {
                return None;
            }

            let trough_pos =
                (search_start..search_end).min_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;

            let weaker_peak = smoothed[left_peak].min(smoothed[right_peak]);
            if smoothed[trough_pos] > weaker_peak * 0.5 {
                return None;
            }

            if trough_pos < self.min_valley_width as usize || trough_pos + self.min_valley_width as usize > n {
                return None;
            }

            Some(profile.x_min + trough_pos as f32)
        })
    }

    /// Find horizontal line (Y-axis) split using index-based partitioning.
    ///
    /// Returns `(above, below)` where `above` holds spans whose rectangle
    /// edge is at larger Y (higher on page in PDF coordinates) and must be
    /// processed first in reading order. PDF Spec ISO 32000-1:2008 §8.3.2.3
    /// defines the default user-space coordinate system with origin at the
    /// lower-left corner and Y increasing upward.
    fn find_vertical_split_indexed(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
    ) -> Option<(Vec<usize>, Vec<usize>)> {
        let profile = self.vertical_projection_indexed(all_spans, indices)?;
        let (valley_start, valley_end, valley_width) = self.find_valley(&profile)?;

        if valley_width < self.min_valley_width {
            return None;
        }

        let split_y = profile.y_min + (valley_start + valley_end) as f32 / 2.0;

        // `Rect::top()` returns `self.y`, the SMALLER Y coordinate of the
        // normalized rectangle — the method name follows a screen-coordinate
        // convention (Y grows downward) but PDF user space has Y growing
        // upward, so in PDF terms `bbox.top()` is actually the LOWER edge of
        // the glyph's bounding box. The predicate `bbox.top() >= split_y`
        // therefore classifies a span into `above` only when its *lowest*
        // point is already above the split line, i.e. the entire span sits
        // above the cut. Since `split_y` is the midpoint of a horizontal
        // projection valley (an empty band by construction), spans should
        // not straddle it in practice; any that do (e.g. a tall header
        // glyph whose ascenders dip into the valley) fall into `below`. ~keep
        let (above, below): (Vec<usize>, Vec<usize>) =
            indices.iter().partition(|&&i| all_spans[i].bbox.top() >= split_y);

        if above.is_empty() || below.is_empty() {
            return None;
        }

        // Row (vertical) splits legitimately produce singleton top
        // partitions for lone headers/titles, so we accept down to 1
        // span per side. The column (horizontal) split is stricter since
        // single-span columns are almost always spurious. ~keep
        let min_side = (indices.len() / 10).max(1);
        if above.len() < min_side || below.len() < min_side {
            return None;
        }

        Some((above, below))
    }

    /// Calculate horizontal projection profile from indexed spans.
    fn horizontal_projection_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Option<ProjectionProfile> {
        if indices.is_empty() {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;

        for &i in indices {
            let span = &all_spans[i];
            x_min = x_min.min(span.bbox.left());
            x_max = x_max.max(span.bbox.right());
            y_min = y_min.min(span.bbox.top());
            y_max = y_max.max(span.bbox.bottom());
        }

        let width = (x_max - x_min).ceil() as usize;
        if width > MAX_PROJECTION_SIZE {
            tracing::warn!(
                width,
                max = MAX_PROJECTION_SIZE,
                "horizontal projection width exceeds MAX_PROJECTION_SIZE, skipping region (degenerate CTM?)"
            );
            return None;
        }
        let mut density = vec![0.0; width];

        // Text extractors frequently over-estimate span bbox widths
        // (trailing whitespace, stretched advance widths). That makes a
        // full-width projection falsely fill the inter-column gutter on
        // multi-column pages. We project each span's TEXT CORE footprint
        // anchored to its LEFT edge (where glyphs actually start), with
        // length proportional to character count. The left edge is
        // reliable; the right edge is not.
        //
        // Additionally, spans whose core width exceeds 55% of the region
        // width are full-width elements (section headers, figure captions,
        // table titles) that span both columns. Including them fills the
        // inter-column gutter in the density array and prevents valley
        // detection. They are excluded from the projection; the column
        // split boundary will still assign them correctly by left edge. ~keep
        let region_width = (x_max - x_min).max(1.0);
        for &i in indices {
            let span = &all_spans[i];
            let height = span.bbox.bottom() - span.bbox.top();
            let char_count = span.text.chars().filter(|c| !c.is_whitespace()).count().max(1);
            // 0.45em per char is a reasonable average across common PDF
            // fonts (Helvetica/Times/Arial at body size) and narrower
            // than the 0.5em advance used for monospace. ~keep
            let approx_char_width = (span.font_size * 0.45).max(2.5);
            let core_width = char_count as f32 * approx_char_width;
            let span_width = span.bbox.right() - span.bbox.left();
            if span_width > region_width * 0.55 {
                continue;
            }
            // Skip isolated single-character/digit spans (table cell values
            // like 'G', 'T', '1', 'A') that scatter across the full X range
            // and fill the column gutter in the density profile. Body text
            // spans always contain multiple characters. ~keep
            if char_count < 2 {
                continue;
            }
            let core_left = span.bbox.left();
            let core_right = (core_left + core_width).min(span.bbox.right());
            let x_start = (core_left - x_min).max(0.0).ceil() as usize;
            let x_end = (core_right - x_min).ceil() as usize;

            for j in x_start..x_end.min(width) {
                density[j] += height;
            }
        }

        Some(ProjectionProfile { density, x_min, y_min })
    }

    /// Calculate vertical projection profile from indexed spans.
    fn vertical_projection_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Option<ProjectionProfile> {
        if indices.is_empty() {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;

        for &i in indices {
            let span = &all_spans[i];
            x_min = x_min.min(span.bbox.left());
            x_max = x_max.max(span.bbox.right());
            y_min = y_min.min(span.bbox.top());
            y_max = y_max.max(span.bbox.bottom());
        }

        let height = (y_max - y_min).ceil() as usize;
        if height > MAX_PROJECTION_SIZE {
            tracing::warn!(
                height,
                max = MAX_PROJECTION_SIZE,
                "vertical projection height exceeds MAX_PROJECTION_SIZE, skipping region (degenerate CTM?)"
            );
            return None;
        }
        let mut density = vec![0.0; height];

        for &i in indices {
            let span = &all_spans[i];
            let y_start = (span.bbox.top() - y_min).max(0.0).ceil() as usize;
            let y_end = (span.bbox.bottom() - y_min).ceil() as usize;
            let w = span.bbox.right() - span.bbox.left();

            for j in y_start..y_end.min(height) {
                density[j] += w;
            }
        }

        Some(ProjectionProfile { density, x_min, y_min })
    }

    /// Find the widest valley (white space gap) in projection profile.
    ///
    /// Only considers INTERIOR valleys — gaps sandwiched between two
    /// non-empty regions. Leading/trailing empty bands (margin space
    /// outside the actual content extent) are ignored; they represent
    /// page margins, not column gutters, and picking them would produce
    /// meaningless splits.
    fn find_valley(&self, profile: &ProjectionProfile) -> Option<(usize, usize, f32)> {
        if profile.density.is_empty() {
            return None;
        }

        let peak = profile.density.iter().copied().fold(0.0, f32::max);

        if peak == 0.0 {
            return None;
        }

        let first_nonzero = profile.density.iter().position(|&d| d > 0.0)?;
        let last_nonzero = profile.density.iter().rposition(|&d| d > 0.0)?;

        let threshold = peak * self.valley_threshold;
        let mut valleys = Vec::new();
        let mut in_valley = false;
        let mut valley_start = 0;

        for (i, &density) in profile.density.iter().enumerate() {
            if density < threshold {
                if !in_valley {
                    valley_start = i;
                    in_valley = true;
                }
            } else if in_valley {
                valleys.push((valley_start, i));
                in_valley = false;
            }
        }

        if in_valley {
            valleys.push((valley_start, profile.density.len()));
        }

        // Merge adjacent interior valley segments separated by a narrow
        // bridge (≤ half the minimum valley width). A callout box or small
        // figure positioned in the column gutter creates a density bump
        // that splits what should be a single valley into two fragments.
        // Bridging re-joins them so the gap is still recognised as a
        // column boundary. ~keep
        let bridge_limit = (self.min_valley_width / 2.0).ceil() as usize;
        let interior: Vec<(usize, usize)> = valleys
            .into_iter()
            .filter(|&(start, end)| start > first_nonzero && end <= last_nonzero + 1)
            .collect();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(interior.len());
        for seg in interior {
            if let Some(last) = merged.last_mut()
                && seg.0 <= last.1 + bridge_limit
            {
                last.1 = last.1.max(seg.1);
                continue;
            }
            merged.push(seg);
        }
        merged
            .into_iter()
            .map(|(start, end)| (start, end, (end - start) as f32))
            .max_by(|a, b| crate::utils::safe_float_cmp(a.2, b.2))
    }

    /// Test-only wrapper for horizontal projection on a contiguous slice.
    #[cfg(test)]
    fn horizontal_projection(&self, spans: &[TextSpan]) -> Option<ProjectionProfile> {
        let indices: Vec<usize> = (0..spans.len()).collect();
        self.horizontal_projection_indexed(spans, &indices)
    }

    /// Test-only wrapper for vertical projection on a contiguous slice.
    #[cfg(test)]
    fn vertical_projection(&self, spans: &[TextSpan]) -> Option<ProjectionProfile> {
        let indices: Vec<usize> = (0..spans.len()).collect();
        self.vertical_projection_indexed(spans, &indices)
    }

    /// Sort spans in reading order (top-to-bottom, left-to-right).
    #[cfg(test)]
    fn sort_spans<'a>(&self, spans: &'a [TextSpan]) -> Vec<&'a TextSpan> {
        let mut sorted: Vec<_> = spans.iter().collect();

        sorted.sort_by(|a, b| {
            let y_cmp = crate::utils::safe_float_cmp(b.bbox.top(), a.bbox.top());
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            crate::utils::safe_float_cmp(a.bbox.left(), b.bbox.left())
        });

        sorted
    }

    /// Sort indices in reading order (top-to-bottom, left-to-right).
    ///
    /// Uses [`crate::utils::row_aware_span_cmp`]'s row-banded baseline
    /// comparator rather than a strict `bbox.top()` sort (GH#1600). A
    /// subscript or superscript run shares its base run's baseline
    /// (`bbox.y`) to within a fraction of a point but is drawn in a
    /// visibly smaller font, so its `top()` (`y + height`) differs from
    /// the base run's by roughly the height difference — several points,
    /// comfortably more than any OTHER same-row cell's `top()` gap. A
    /// strict `top()` sort therefore treats the subscript as a separate,
    /// lower "line" and can insert an unrelated same-row cell between a
    /// base glyph and its own subscript. `row_aware_span_cmp` quantizes Y
    /// into `ROW_BAND_TOLERANCE_PT`-wide bands before comparing, so runs
    /// sharing a baseline band stay ordered by X regardless of height. ~keep
    fn sort_indices(&self, all_spans: &[TextSpan], indices: &[usize]) -> Vec<usize> {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_by(|&a, &b| {
            crate::utils::row_aware_span_cmp(
                all_spans[a].bbox.y,
                all_spans[a].bbox.x,
                all_spans[b].bbox.y,
                all_spans[b].bbox.x,
            )
        });
        sorted
    }
}

/// Internal projection profile representation.
struct ProjectionProfile {
    /// Density values (height or width accumulated per bin)
    density: Vec<f32>,

    /// Origin coordinates
    x_min: f32,
    y_min: f32,
}

impl ReadingOrderStrategy for XYCutStrategy {
    fn apply(&self, spans: Vec<TextSpan>, _context: &ReadingOrderContext) -> Result<Vec<OrderedTextSpan>> {
        // Detects multi-line heading runs and routes the
        // partition through synthetic-span space so the splitter treats
        // each wrapped heading as a single atomic block. When no
        // headings are found we use the original index-only path that
        // avoids span clones during recursion. ~keep
        let heading_runs = self.find_heading_runs(&spans);

        let index_groups: Vec<Vec<usize>> = if heading_runs.is_empty() {
            let indices: Vec<usize> = (0..spans.len()).collect();
            self.partition_indexed(&spans, &indices)
        } else {
            let (synthetic, synthetic_origin) = self.synthesize_for_partition(&spans, &heading_runs);
            let synth_indices: Vec<usize> = (0..synthetic.len()).collect();
            let synth_groups = self.partition_indexed(&synthetic, &synth_indices);
            // Project synthetic-space groups back to ORIGINAL-span
            // indices (so the move-out below works on the input Vec). ~keep
            synth_groups
                .into_iter()
                .map(|group| {
                    let mut out = Vec::with_capacity(group.len());
                    for synth_idx in group {
                        out.extend(synthetic_origin[synth_idx].iter().copied());
                    }
                    out
                })
                .collect()
        };

        // Build result — moves spans out by index (no extra clone) ~keep
        let mut ordered = Vec::with_capacity(spans.len());
        // Convert spans to indexable storage for O(1) moves ~keep
        let mut span_slots: Vec<Option<TextSpan>> = spans.into_iter().map(Some).collect();
        let mut order_index = 0usize;

        for (group_idx, group) in index_groups.iter().enumerate() {
            for &i in group {
                if let Some(span) = span_slots[i].take() {
                    ordered.push(
                        OrderedTextSpan::with_info(span, order_index, ReadingOrderInfo::xycut()).with_group(group_idx),
                    );
                    order_index += 1;
                }
            }
        }

        Ok(ordered)
    }

    fn name(&self) -> &'static str {
        "XYCutStrategy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn make_span(x: f32, y: f32, width: f32, height: f32) -> TextSpan {
        make_span_text(x, y, width, height, "test", 12.0)
    }

    /// Like make_span but with realistic body-text density (~72 non-whitespace chars
    /// at 12pt, matching a full Letter-width column). Used when is_single_column_region
    /// must correctly identify a wide single-column page as not multi-column.
    fn make_body_span(x: f32, y: f32, width: f32, height: f32) -> TextSpan {
        // 72 non-whitespace characters at 12pt → core_width = 72 × 5.4 = 388.8pt
        // which is 83% of a 468pt column — enough to pass the 80% dense check. ~keep
        let text = "abcdefghijklmnopqrstuvwxyz".repeat(3); // 78 non-whitespace chars ~keep
        make_span_text(x, y, width, height, &text, 12.0)
    }

    fn make_span_text(x: f32, y: f32, width: f32, height: f32, text: &str, font_size: f32) -> TextSpan {
        use crate::layout::{Color, FontWeight};

        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: Rect::new(x, y, width, height),
            font_size,
            font_name: "Arial".to_string(),
            font_weight: FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: Color { r: 0.0, g: 0.0, b: 0.0 },
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    #[test]
    fn test_single_column_no_split() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(10.0, 85.0, 50.0, 10.0),
            make_span(10.0, 70.0, 50.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    /// Realistic A4/Letter single-column page: 60 lines of body text,
    /// 14pt leading, one paragraph gap (30pt) mid-page. Only one body
    /// column exists, so XY-Cut must return exactly one group and
    /// preserve top-to-bottom reading order. A density-dip split at the
    /// paragraph gap would fragment the page and non-monotonically
    /// interleave paragraph contents.
    #[test]
    fn test_single_column_body_text_no_fragmentation() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        let line_height = 12.0;
        let leading = 14.0;
        let left = 72.0;
        let right = 540.0;
        let width = right - left;
        let mut y = 720.0;
        for i in 0..60 {
            // Insert a paragraph gap in the middle (30pt, larger than min_valley_width=15pt) ~keep
            if i == 30 {
                y -= 30.0;
            }
            // Use realistic body text density (78 non-whitespace chars at 12pt) so
            // is_single_column_region correctly classifies the region as single-column. ~keep
            spans.push(make_body_span(left, y, width, line_height));
            y -= leading;
        }

        let groups = strategy.partition_region(&spans);
        assert_eq!(
            groups.len(),
            1,
            "single-column body text must not be split by XY-Cut (got {} groups)",
            groups.len()
        );
        assert_eq!(groups[0].len(), 60, "all 60 spans must be preserved");

        let mut last_y = f32::MAX;
        for s in &groups[0] {
            assert!(
                s.bbox.top() <= last_y + 0.01,
                "reading order must be top-to-bottom: {} > {}",
                s.bbox.top(),
                last_y
            );
            last_y = s.bbox.top();
        }
    }

    /// After a vertical (row) split, the partition at higher Y (top of
    /// page in PDF coords) must be processed first in reading order so
    /// that header content appears before body content.
    #[test]
    fn test_vertical_split_preserves_top_to_bottom_order() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        // Two columns at y ∈ {200, 180, 160} (body), header at y=400.
        // Horizontal split will find the column gutter first; within each
        // column the header must still come out first in reading order. ~keep
        let spans = vec![
            make("HEADER LEFT", 50.0, 400.0, 200.0),
            make("HEADER RIGHT", 300.0, 400.0, 200.0),
            make("body-L1", 50.0, 200.0, 150.0),
            make("body-R1", 300.0, 200.0, 150.0),
            make("body-L2", 50.0, 180.0, 150.0),
            make("body-R2", 300.0, 180.0, 150.0),
        ];
        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        let texts: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();
        assert!(
            texts[0].contains("HEADER"),
            "expected HEADER first, got sequence {:?}",
            texts
        );
    }

    /// Single-column page with a tall header band ("Title" or "Chapter
    /// heading") at the top. XY-Cut may validly split the header from
    /// the body (vertical Y-split) but must not further split the body
    /// into per-paragraph chunks.
    #[test]
    fn test_single_column_with_header_at_most_two_groups() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();

        spans.push(make_span(72.0, 750.0, 468.0, 24.0));

        let mut y = 670.0;
        for _ in 0..40 {
            spans.push(make_span(72.0, y, 468.0, 12.0));
            y -= 14.0;
        }

        let groups = strategy.partition_region(&spans);
        assert!(
            groups.len() <= 2,
            "single-column with header should produce at most 2 groups, got {}",
            groups.len()
        );
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, 41);
    }

    #[test]
    fn test_two_column_split() {
        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let spans = vec![
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(10.0, 85.0, 50.0, 10.0),
            make_span(100.0, 100.0, 50.0, 10.0),
            make_span(100.0, 85.0, 50.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans);
        assert!(!groups.is_empty(), "Expected at least 1 group");
        let total_spans: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total_spans, 4, "Expected all 4 spans to be preserved");
    }

    /// GH#1600. A subscript run (shorter height, baseline dropped a
    /// fraction of a point below its base run) must sort immediately after
    /// its base run, not after a sibling cell in the next column whose
    /// `top()` happens to land between them.
    ///
    /// Geometry lifted from the reporter's reproducer (`Q`/`HE`/`GJ`
    /// row): base "Q" at y=612.13 h=11.59 (top=623.72), subscript "HE" at
    /// y=611.50 h=7.34 (top=618.84 — baseline only 0.63pt below the base,
    /// but top() differs by ~4.9pt because subscript glyphs are drawn in a
    /// visibly smaller font), unit cell "GJ" in the next column at
    /// y=612.13 h=11.59 (top=623.72, tied with the base). Sorting by
    /// `top()` descending places GJ (tied top, lower x than nothing to its
    /// left) ahead of HE, tearing "QHE" into "Q" ... "HE" with "GJ" wedged
    /// between them. All three share one baseline band (`ROW_BAND_TOLERANCE_PT`
    /// = 3.0pt covers the 0.63pt baseline gap easily), so a baseline-aware,
    /// row-banded comparator keeps Q and HE adjacent. ~keep
    #[test]
    fn test_subscript_sorts_immediately_after_base_not_after_next_column() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(206.74, 612.13, 8.20, 11.59, "Q", 11.59),
            make_span_text(212.76, 611.50, 6.83, 7.34, "HE", 7.34),
            make_span_text(253.33, 612.13, 12.08, 11.59, "GJ", 11.59),
        ];

        let groups = strategy.partition_region(&spans);
        let texts: Vec<&str> = groups.iter().flatten().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["Q", "HE", "GJ"],
            "subscript HE must stay adjacent to base Q, ahead of the next column's GJ"
        );
    }

    /// GH#1600 negative control. Two ordinary body-text lines at a real
    /// line-height apart (14pt — typical single-spaced 12pt body leading)
    /// must NOT be treated as one row band and interleaved by X, even
    /// though they land in the same tiny (`n < min_spans_for_split`)
    /// region that reaches `sort_indices`. `ROW_BAND_TOLERANCE_PT` is
    /// 3.0pt; a 14pt gap is 4.6x that, so the two lines fall into
    /// different bands and stay in top-to-bottom, row-major order. This
    /// guards the fix above from overreaching: the row-band tolerance is
    /// narrow enough to keep a subscript with its base (0.63pt baseline
    /// gap) without also merging two genuinely separate lines whose
    /// columns would otherwise look identical to the subscript case (each
    /// side narrower than `MIN_RESULT_WIDTH_PT`, so no column split is
    /// found and both lines land in the same `sort_indices` call). ~keep
    #[test]
    fn test_row_band_does_not_merge_two_distinct_lines() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(10.0, 200.0, 50.0, 10.0, "A1", 10.0),
            make_span_text(100.0, 200.0, 50.0, 10.0, "A2", 10.0),
            make_span_text(10.0, 186.0, 50.0, 10.0, "B1", 10.0),
            make_span_text(100.0, 186.0, 50.0, 10.0, "B2", 10.0),
        ];

        let groups = strategy.partition_region(&spans);
        let texts: Vec<&str> = groups.iter().flatten().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["A1", "A2", "B1", "B2"],
            "two distinct 14pt-apart lines must stay in row-major order, not interleave by X"
        );
    }

    #[test]
    fn test_three_column_layout() {
        let strategy = XYCutStrategy::new();
        // Realistic column widths (≥ 60 pt per column, ≥ 6 body chars at
        // 10 pt — find_horizontal_split rejects narrower splits since
        // body columns are never sliver-wide). ~keep
        let spans = vec![
            make_span(10.0, 100.0, 100.0, 10.0),
            make_span(10.0, 85.0, 100.0, 10.0),
            make_span(180.0, 100.0, 100.0, 10.0),
            make_span(180.0, 85.0, 100.0, 10.0),
            make_span(350.0, 100.0, 100.0, 10.0),
            make_span(350.0, 85.0, 100.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans);
        assert!(groups.len() >= 2, "Expected at least 2 groups, got {}", groups.len());
    }

    #[test]
    fn test_small_region_no_split() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 50.0, 10.0)];

        let groups = strategy.partition_region(&spans);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 1);
    }

    #[test]
    fn test_sort_order() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span(100.0, 70.0, 50.0, 10.0),
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(100.0, 100.0, 50.0, 10.0),
            make_span(10.0, 70.0, 50.0, 10.0),
        ];

        let sorted = strategy.sort_spans(&spans);

        assert_eq!(sorted[0].bbox.top(), 100.0);
        assert_eq!(sorted[0].bbox.left(), 10.0);
        assert_eq!(sorted[1].bbox.top(), 100.0);
        assert_eq!(sorted[1].bbox.left(), 100.0);
    }

    #[test]
    fn test_horizontal_projection() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 30.0, 10.0), make_span(100.0, 100.0, 30.0, 10.0)];

        if let Some(profile) = strategy.horizontal_projection(&spans) {
            assert!(!profile.density.is_empty());
            assert!(profile.density.len() >= 120);

            let gap_start = 30;
            let gap_end = 90;
            if gap_end <= profile.density.len() {
                let gap_region = &profile.density[gap_start..gap_end];
                let gap_density: f32 = gap_region.iter().sum();
                assert!(gap_density < 1.0);
            }
        }
    }

    #[test]
    fn test_vertical_projection() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 50.0, 20.0), make_span(10.0, 50.0, 50.0, 20.0)];

        if let Some(profile) = strategy.vertical_projection(&spans) {
            assert!(!profile.density.is_empty());
            assert!(profile.density.len() > 50);
        }
    }

    #[test]
    fn test_narrow_gap_rejected() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 30.0, 10.0), make_span(45.0, 100.0, 30.0, 10.0)];

        let groups = strategy.partition_region(&spans);
        assert_eq!(groups.len(), 1);
    }

    /// Regression test for Bug 2: degenerate CTM places spans at ~100 trillion PDF points.
    /// horizontal_projection_indexed must return None instead of attempting a
    /// ~100-trillion-element vec allocation (which triggers handle_alloc_error → abort).
    #[test]
    fn test_degenerate_ctm_horizontal_projection_returns_none() {
        let strategy = XYCutStrategy::new();
        // Observed crash coordinate: 99_992_777_785_344 PDF points on a ~3968-point page. ~keep
        let degenerate_x: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(degenerate_x, 100.0, 30.0, 10.0),
        ];

        let result = strategy.horizontal_projection(&spans);
        assert!(
            result.is_none(),
            "expected None for projection spanning ~100 trillion points, got Some"
        );
    }

    /// Vertical projection must also return None for degenerate CTM y-coordinates.
    #[test]
    fn test_degenerate_ctm_vertical_projection_returns_none() {
        let strategy = XYCutStrategy::new();
        let degenerate_y: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(10.0, degenerate_y, 30.0, 10.0),
        ];

        let result = strategy.vertical_projection(&spans);
        assert!(
            result.is_none(),
            "expected None for projection spanning ~100 trillion points, got Some"
        );
    }

    /// A CENTERED title/subtitle/byline block (each line
    /// centered, scattered leftmost edges) must NOT be split into
    /// per-word "columns". The centered "Quarterly Inventory Review"
    /// title (3 large words at the same Y with wide gaps) plus centered
    /// subtitle/byline previously aligned accidentally into fake columns,
    /// scrambling reading order. The centered-block guard must keep the
    /// whole block as ONE group so the title line stays intact.
    #[test]
    fn test_issue1_centered_title_block_not_split_into_columns() {
        let strategy = XYCutStrategy::new();
        // Centered title (y=612, fs=28), subtitle (y=572), byline (y=532).
        // Leftmost edges scattered: 145 / 185 / 210 (centered, not columnar). ~keep
        let spans = vec![
            make_span_text(145.0, 612.0, 115.0, 28.0, "Quarterly", 28.0),
            make_span_text(300.0, 612.0, 115.0, 28.0, "Inventory", 28.0),
            make_span_text(430.0, 612.0, 92.0, 28.0, "Review", 28.0),
            make_span_text(185.0, 572.0, 40.0, 14.0, "Spring", 14.0),
            make_span_text(238.0, 572.0, 31.0, 14.0, "2025", 14.0),
            make_span_text(300.0, 572.0, 70.0, 14.0, "Distribution", 14.0),
            make_span_text(210.0, 532.0, 45.0, 10.0, "Northwind", 10.0),
            make_span_text(290.0, 532.0, 34.0, 10.0, "Traders", 10.0),
        ];
        let groups = strategy.partition_region(&spans);
        assert_eq!(
            groups.len(),
            1,
            "centered title block must stay one group, got {} groups",
            groups.len()
        );
        let g0: Vec<&str> = groups[0].iter().map(|s| s.text.as_str()).collect();
        let qi = g0.iter().position(|t| *t == "Quarterly").unwrap();
        let ii = g0.iter().position(|t| *t == "Inventory").unwrap();
        let ri = g0.iter().position(|t| *t == "Review").unwrap();
        assert!(qi < ii && ii < ri, "title words out of order: {:?}", g0);
    }

    /// XYCut must assign distinct group_id values to spans in different
    /// spatial partitions so that converters can keep each column's content
    /// contiguous instead of interleaving by Y-coordinate.
    #[test]
    fn test_xycut_group_id_two_column_layout() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        let spans = vec![
            make("Description", 50.0, 100.0, 150.0),
            make("Amount", 400.0, 100.0, 150.0),
            make("Widget A", 50.0, 120.0, 150.0),
            make("$150.00", 400.0, 120.0, 150.0),
            make("Widget B", 50.0, 140.0, 150.0),
            make("Discount", 400.0, 140.0, 150.0),
            make("$25.00", 400.0, 160.0, 150.0),
        ];

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        assert!(
            ordered.iter().all(|s| s.group_id.is_some()),
            "all spans should have group_id set by XYCut"
        );

        let left_groups: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() < 300.0)
            .map(|s| s.group_id.unwrap())
            .collect();
        let right_groups: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() >= 300.0)
            .map(|s| s.group_id.unwrap())
            .collect();

        assert!(
            left_groups.windows(2).all(|w| w[0] == w[1]),
            "left column spans should share the same group_id: {:?}",
            left_groups
        );
        assert!(
            right_groups.windows(2).all(|w| w[0] == w[1]),
            "right column spans should share the same group_id: {:?}",
            right_groups
        );

        assert_ne!(
            left_groups[0], right_groups[0],
            "left and right columns should have different group_ids"
        );

        let left_orders: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() < 300.0)
            .map(|s| s.reading_order)
            .collect();
        let right_orders: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() >= 300.0)
            .map(|s| s.reading_order)
            .collect();
        let left_max = *left_orders.iter().max().unwrap();
        let right_min = *right_orders.iter().min().unwrap();
        let left_min = *left_orders.iter().min().unwrap();
        let right_max = *right_orders.iter().max().unwrap();
        assert!(
            left_max < right_min || right_max < left_min,
            "columns must be contiguous in reading order: left={:?} right={:?}",
            left_orders,
            right_orders
        );
    }

    /// Plain-text rendering must keep group_id-separated columns as
    /// contiguous blocks, not interleave them by Y-coordinate.
    #[test]
    fn test_group_id_plain_text_no_interleave() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        let spans = vec![
            make("Description", 50.0, 100.0, 150.0),
            make("Amount", 400.0, 100.0, 150.0),
            make("Widget A", 50.0, 120.0, 150.0),
            make("$150.00", 400.0, 120.0, 150.0),
            make("Widget B", 50.0, 140.0, 150.0),
            make("Discount", 400.0, 140.0, 150.0),
            make("$25.00", 400.0, 160.0, 150.0),
        ];

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        for expected in ["Description", "Amount", "Widget A", "$150.00"] {
            assert!(order.contains(&expected), "missing {expected:?}: {order:?}");
        }

        // Each group_id-separated column must occupy one contiguous run of the
        // reading order. Interleaving them by Y — the defect this guards — would
        // scatter each column's spans through the other's. ~keep
        let run_is_contiguous = |members: &[&str]| {
            let mut indices: Vec<usize> = members
                .iter()
                .map(|t| order.iter().position(|s| s == t).expect("span present"))
                .collect();
            indices.sort_unstable();
            indices.windows(2).all(|w| w[1] == w[0] + 1)
        };
        assert!(
            run_is_contiguous(&["Description", "Widget A", "Widget B"]),
            "left column must be one contiguous run: {order:?}"
        );
        assert!(
            run_is_contiguous(&["Amount", "$150.00", "Discount", "$25.00"]),
            "right column must be one contiguous run: {order:?}"
        );
    }

    /// Builder for a bold heading span at a given font size. Used by the
    /// fix-543 tests to construct the "bold/large-font run spanning ≥ 2
    /// lines" shape the pre-partition heading lock must catch.
    fn make_bold_span(x: f32, y: f32, width: f32, text: &str, font_size: f32) -> TextSpan {
        use crate::layout::FontWeight;
        let mut s = make_span_text(x, y, width, font_size, text, font_size);
        s.font_weight = FontWeight::Bold;
        s
    }

    /// fix-543 unit: `find_heading_runs` must detect a 2-line bold
    /// heading whose wrapped tail line sits below the first line with
    /// matching X-extent. Single-line bold spans or paragraph-gap
    /// shapes must NOT be returned.
    #[test]
    fn find_heading_runs_detects_2_line_bold_heading() {
        let strategy = XYCutStrategy::new();

        // Body baseline (12pt regular) — establishes the median. ~keep
        let mut spans = Vec::new();
        let body_left = 72.0;
        let body_width = 200.0;
        let mut y = 720.0;
        for _ in 0..10 {
            spans.push(make_body_span(body_left, y, body_width, 12.0));
            y -= 14.0;
        }

        spans.push(make_bold_span(body_left, 500.0, 180.0, "2.3 Performance and", 14.0));
        spans.push(make_bold_span(
            body_left,
            484.0,
            180.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));

        let runs = strategy.find_heading_runs(&spans);
        assert_eq!(runs.len(), 1, "expected exactly one heading run, got {runs:?}");
        assert_eq!(
            runs[0].span_indices.len(),
            2,
            "expected the run to cover both heading lines"
        );

        // A LONE bold span (no second line) must NOT be locked: that
        // case is a single-line heading that XY-cut already handles. ~keep
        let mut spans_single = vec![make_body_span(body_left, 720.0, body_width, 12.0); 5];
        spans_single.push(make_bold_span(body_left, 500.0, 180.0, "Lone Heading", 14.0));
        let runs_single = strategy.find_heading_runs(&spans_single);
        assert!(
            runs_single.is_empty(),
            "single-line bold runs must not produce a HeadingRun"
        );
    }

    /// fix-543 unit: the canonical repro shape — left-column 2-line
    /// bold heading whose wrapped tail line Y-overlaps right-column
    /// dense content (table caption + rows). Pre-fix, line 2 of the
    /// heading was bucketed into the RIGHT block; post-fix the lock
    /// keeps both heading lines in the LEFT block, adjacent to the
    /// left-column body paragraph.
    #[test]
    fn partition_keeps_heading_in_left_block() {
        let strategy = XYCutStrategy::new();

        let left_col_x = 72.0_f32;
        let right_col_x = 362.0_f32;
        let col_width = 260.0_f32;

        let mut spans = Vec::new();

        // Left column: 2-line bold heading at Y=500/484, then 8 body
        // lines below at Y=460..360 (so the body paragraph anchors the
        // left block in reading order). ~keep
        spans.push(make_bold_span(left_col_x, 500.0, 180.0, "2.3 Performance and", 14.0));
        spans.push(make_bold_span(
            left_col_x,
            484.0,
            220.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));
        let mut y = 460.0_f32;
        for _ in 0..8 {
            spans.push(make_body_span(left_col_x, y, col_width, 12.0));
            y -= 14.0;
        }

        // Right column: dense table-caption-style content that
        // Y-overlaps the heading's second line (Y=484). The
        // pre-fix block-assignment step pulled the heading's tail
        // into THIS column because the geometry was alone in
        // deciding bucket membership. ~keep
        spans.push(make_body_span(right_col_x, 500.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 484.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 468.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 452.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 436.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 420.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 404.0, col_width, 12.0));

        let groups = strategy.partition_region(&spans);

        let heading_first_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.contains("2.3 Performance and")))
            .expect("heading line 1 must land in some group");
        let heading_second_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.contains("Advantages of Vari-linear Network")))
            .expect("heading line 2 must land in some group");

        assert_eq!(
            heading_first_group, heading_second_group,
            "both heading lines must end up in the SAME block — pre-fix \
             they split across left/right column blocks"
        );

        let group = &groups[heading_first_group];
        for s in group {
            assert!(
                s.bbox.left() < right_col_x,
                "heading + body group must stay in the LEFT column; \
                 stray span at x={} (right_col starts at {}): {:?}",
                s.bbox.left(),
                right_col_x,
                s.text
            );
        }
    }

    /// fix-543 unit: a wrapped heading's tail line must stay in its own
    /// column block. Pre-fix the tail was bucketed into the right-hand
    /// column and ordered after that column's body, orphaning it from the
    /// heading it belongs to.
    #[test]
    fn wrapped_heading_tail_stays_in_its_column_block() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();

        let left_col_x = 72.0_f32;
        let right_col_x = 362.0_f32;
        let col_width = 260.0_f32;

        let mut spans = Vec::new();
        spans.push(make_bold_span(left_col_x, 500.0, 180.0, "Performance and", 14.0));
        spans.push(make_bold_span(
            left_col_x,
            484.0,
            220.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));
        let mut y = 460.0_f32;
        for _ in 0..6 {
            spans.push(make_body_span(left_col_x, y, col_width, 12.0));
            y -= 14.0;
        }
        for ky in [500.0, 484.0, 468.0, 452.0, 436.0, 420.0] {
            spans.push(make_body_span(right_col_x, ky, col_width, 12.0));
        }

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).expect("apply");
        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        // Both heading halves must appear, and BOTH must precede any
        // right-column content. Pre-fix, the wrapped-heading tail was
        // bucketed into the right-column block and emitted AFTER the
        // right column's body, then promoted to a fresh heading level
        // by `heading_level_ratio` (since it lost its body
        // continuation) — that's the phantom `### …` in the wrong
        // location. Post-fix the lock keeps both heading lines in the
        // left block adjacent to each other. ~keep
        let pos_first = order
            .iter()
            .position(|t| t.contains("Performance and"))
            .expect("heading line 1 must appear in reading order");
        let pos_second = order
            .iter()
            .position(|t| t.contains("Advantages of Vari-linear Network"))
            .expect("heading line 2 must appear in reading order");

        assert_eq!(
            pos_second,
            pos_first + 1,
            "the wrapped heading's two lines must stay adjacent: {order:?}"
        );
        // Both heading lines belong at the very top of the left-column
        // block, not floating somewhere after the right-column body.
        // (Pre-fix the orphan tail landed deep into the document.) ~keep
        let cap = ((order.len() as f32) * 0.30) as usize;
        assert!(
            pos_second < cap.max(2),
            "heading-line-2 ordered late — likely the pre-fix \
             orphan-in-wrong-column behaviour. pos_second={pos_second}, \
             cap={cap}, order={order:?}"
        );
    }

    /// GH#1738: a numbered heading whose producer set the marker and title
    /// in ONE `TJ` array with a kern for the tab (`[(3.)-1329.5(Title )] TJ`)
    /// must not absorb a right-column caption that Y-overlaps its first
    /// line. The kern becomes a space-only span that is always
    /// `FontWeight::Normal` (see `extractors/text/advance.rs`), which used
    /// to break `find_heading_runs`'s clustering right at the
    /// marker/title boundary: the marker ("3.") was left out of the run
    /// while the wrapped title ("…GASTECHNISCHE" / "INSTALLATEUR")
    /// formed a run on its own, whose narrower union bbox no longer
    /// covered the marker's column position and let the caption's span
    /// land between the run's two original lines once expanded.
    ///
    /// Geometry transcribed verbatim from `PdfDocument::extract_spans` on
    /// page 1 of the GH#1738 reproducer (a two-column A4 page: a bold
    /// numbered heading opening the top of the left column, a bold
    /// caption at the top of the right column, and a body underneath
    /// each) — `x`, `y` (top-origin) and `font_size` are the measured
    /// values, and `y` decreases top-to-bottom exactly like every other
    /// `y` in this file's `dense_two_column_*` fixtures. No position is
    /// invented. ~keep
    #[test]
    fn gh1738_kern_tab_heading_does_not_absorb_other_column_caption() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();

        let kern_space = |x: f32, y: f32, width: f32, font_size: f32| {
            let mut s = make_span_text(x, y, width, font_size, " ", font_size);
            s.offset_semantic = true;
            s
        };
        let body = |x: f32, y: f32, width: f32, text: &str| make_span_text(x, y, width, 8.3, text, 8.3);

        #[rustfmt::skip]
        let spans = vec![
            make_bold_span(30.07, 809.09, 7.51, "3.", 9.0),
            kern_space(37.58, 809.09, 0.28, 9.0),
            make_bold_span(312.60, 810.40, 129.00, "Fig. 6. Branderdruk (P1-P2)", 10.0),
            make_bold_span(49.54, 809.09, 188.02, "INSTRUCTIES VOOR DE GASTECHNISCHE ", 9.0),
            make_bold_span(49.54, 799.39, 67.66, "INSTALLATEUR", 9.0),
            make_bold_span(30.07, 782.02, 12.51, "3.1", 9.0),
            kern_space(42.58, 782.02, 0.28, 9.0),
            make_bold_span(49.54, 782.02, 158.68, "GASAANSLUITING EN INSTALLATIE.", 9.0),
            body(30.10, 764.00, 6.92, "1."),
            body(42.80, 764.00, 224.46, "Werk altijd volgens de laatste eisen van de geldende normen"),
            body(42.80, 755.00, 116.33, "en de plaatselijke voorschriften."),
            body(30.10, 737.00, 6.92, "2."),
            body(42.80, 737.00, 202.33, "Plaats bij te verwachten vuil in het gas bij voorkeur een"),
            body(42.80, 728.00, 78.05, "gaszeef in de leiding."),
            body(30.10, 710.00, 6.92, "3."),
            body(42.80, 710.00, 225.82, "Als het gasblok op dichtheid wordt gecontroleerd, gebeurt dat"),
            body(42.80, 701.00, 189.36, "met een druk van ten hoogste 500 mm waterkolom."),
            body(30.10, 683.00, 6.92, "4."),
            body(42.80, 683.00, 223.45, "De fabrieksafstelling van de tweetrapsregeling bedraagt 21,6"),
            body(42.80, 674.00, 224.47, "kW voor warm water en 14 kW voor de verwarming. Voor het"),
            body(42.80, 665.00, 219.31, "aanpassen van de verwarmingsinstelling, zie het kopje over"),
            body(42.80, 656.00, 199.91, "de tweetrapsregeling. De branderdruk is de uitlaatdruk"),
            body(42.80, 647.00, 220.75, "gemeten ten opzichte van de vuurhaarddruk. Voor de plaats"),
            body(42.80, 638.00, 213.45, "van de meetnippels, zie fig. 5. Sluit voor het meten van de"),
            body(42.80, 629.00, 210.56, "branderdruk de slangen van de drukverschilmeter aan op"),
            body(42.80, 620.00, 196.75, "beide meetnippels. In fig. 6 is het nominale vermogen"),
            body(42.80, 611.00, 113.12, "uitgezet tegen de branderdruk."),
            make_bold_span(30.10, 519.00, 135.65, "Fig. 5. Branderdrukinstelling", 10.0),
            make_bold_span(312.60, 518.40, 79.52, "Tweetrapsregeling", 9.0),
            body(312.60, 500.30, 227.13, "Als de verwarmingsinstallatie meer of minder vermogen nodig"),
            body(312.60, 491.30, 234.65, "heeft dan de fabrieksafstelling van 14 kW, kan de capaciteit van"),
            body(312.60, 482.30, 223.06, "de ketel met de tweetrapsregeling worden aangepast aan de"),
            body(312.60, 473.30, 220.77, "installatie. Op de fabriek wordt de hoogste belasting voor de"),
            body(312.60, 464.30, 234.97, "warmwatervoorziening afgesteld op het nominale vermogen. De"),
            body(312.60, 455.30, 197.63, "tweetrapsregeling (zie fig. 7) wordt als volgt ingesteld:"),
            body(312.60, 446.30, 87.90, "a. Spoel onbekrachtigd."),
            body(322.50, 437.30, 225.84, "Lage belasting instellen met de zeskante stelschroef A (let op"),
            body(322.50, 428.30, 128.87, "dat deze vrij ligt van stelschroef B)."),
            body(312.60, 419.30, 76.36, "b. Spoel bekrachtigd"),
            body(322.50, 410.30, 205.03, "Hoogste belasting controleren en zo nodig afstellen met"),
            body(322.50, 401.30, 50.31, "stelschroef B."),
            body(322.50, 392.30, 191.66, "Stift tegenhouden met een inbussleutel van 2,5 mm."),
            body(312.60, 383.30, 94.84, "c. Stelschroef A aflakken."),
            body(312.60, 374.30, 115.11, "d. Branderdrukken controleren."),
            make_bold_span(312.60, 293.20, 120.10, "Fig. 7. Tweetrapsregeling", 10.0),
        ];

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).expect("apply");
        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        let pos_marker = order
            .iter()
            .position(|&t| t == "3.")
            .expect("the heading marker must appear in reading order");
        let pos_title = order
            .iter()
            .position(|&t| t == "INSTRUCTIES VOOR DE GASTECHNISCHE ")
            .expect("the heading title must appear in reading order");
        let pos_wrap = order
            .iter()
            .position(|&t| t == "INSTALLATEUR")
            .expect("the heading's wrapped second line must appear in reading order");
        let pos_caption = order
            .iter()
            .position(|&t| t == "Fig. 6. Branderdruk (P1-P2)")
            .expect("the other column's caption must appear in reading order");

        assert_eq!(
            pos_title,
            pos_marker + 1,
            "the marker and title must stay adjacent: {order:?}"
        );
        assert_eq!(
            pos_wrap,
            pos_title + 1,
            "the wrapped second line must stay adjacent to the title, not have \
             the other column's caption spliced in between: {order:?}"
        );
        assert!(
            pos_caption < pos_marker || pos_caption > pos_wrap,
            "the other column's caption must not land inside the heading run \
             (marker={pos_marker}, title={pos_title}, wrap={pos_wrap}, \
             caption={pos_caption}): {order:?}"
        );
    }

    /// A 2-column body where the gutter is narrower than
    /// `min_valley_width` AND the line-start cluster shape carries
    /// outlier singletons (title / caption / equation labels) so
    /// `detect_two_column_prose` bails on `clusters.len() != 2`.
    /// The narrow-gutter prose detector should catch this via
    /// gap-position clustering.
    #[test]
    fn test_narrow_gutter_prose_with_outlier_singletons() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        let mut spans = Vec::new();
        // 14 body lines with a tight gutter at x ≈ 295 (gap is
        // ~10 pt: left column ends at ~285, right column starts at
        // ~305). Each side has multiple per-word spans so the line
        // density is realistic. ~keep
        for i in 0..14 {
            let y = 600.0 - (i as f32) * 14.0;
            let left_words = ["Dwarf", "spheroidal", "galaxies", "of", "the", "Local", "Group", "are"];
            let mut x = 40.0;
            for w in left_words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
            let right_words = [
                "The",
                "Schwarzschild",
                "modeling",
                "technique",
                "offers",
                "another",
                "approach",
                "to",
            ];
            let mut x = 305.0;
            for w in right_words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
        }
        // Outlier singletons (title / caption / equation labels)
        // whose left edges don't align with either column. Under
        // detect_two_column_prose these produce extra clusters and
        // block detection — the narrow-gutter detector should still
        // catch the body via gap-position clustering. ~keep
        spans.push(make_word(145.0, 700.0, "Title text spanning"));
        spans.push(make_word(214.0, 680.0, "Caption that wraps somewhere"));
        spans.push(make_word(455.0, 670.0, "(1)"));
        spans.push(make_word(505.0, 660.0, "(2)"));

        let groups = strategy.partition_region(&spans);
        assert!(
            groups.len() >= 2,
            "expected at least 2 groups (column split) for narrow-gutter 2-col body \
             with outlier singletons; got {} group(s)",
            groups.len()
        );

        for (gi, g) in groups.iter().enumerate() {
            let has_left = g.iter().any(|s| s.bbox.left() < 200.0);
            let has_right = g.iter().any(|s| s.bbox.left() >= 305.0);
            assert!(
                !(has_left && has_right),
                "group {} contains spans from both columns — the column split did \
                 not separate them: {:?}",
                gi,
                g.iter().map(|s| (s.text.clone(), s.bbox.left())).collect::<Vec<_>>()
            );
        }
    }

    /// Negative: a single-column body with one large figure caption
    /// produces a strong within-line gap on the caption row but no
    /// recurring gap pattern across body lines. The narrow-gutter
    /// detector must NOT fire (would scramble reading order).
    #[test]
    fn test_narrow_gutter_prose_negative_single_col_with_caption() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        let mut spans = Vec::new();
        for i in 0..14 {
            let y = 600.0 - (i as f32) * 14.0;
            let words = [
                "This",
                "is",
                "an",
                "ordinary",
                "single",
                "column",
                "body",
                "paragraph",
                "with",
                "no",
                "interior",
                "gutter",
                "or",
                "wide",
                "gap",
            ];
            let mut x = 40.0;
            for w in words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
        }
        // One row that DOES have a within-line gap (figure caption
        // with a label on the right). This single outlier must not
        // make the page look 2-column. ~keep
        spans.push(make_word(40.0, 410.0, "Figure"));
        spans.push(make_word(80.0, 410.0, "caption"));
        spans.push(make_word(300.0, 410.0, "(continued)"));

        let groups = strategy.partition_region(&spans);
        // For a true single-column page, partition_region should
        // return either ONE group or a small number from row/header
        // splits — never a column split that lands left-side spans
        // in one group and right-side spans in another.
        // Count groups that contain at least one body span (x < 100): ~keep
        let body_groups = groups
            .iter()
            .filter(|g| g.iter().any(|s| s.bbox.left() < 100.0 && s.text != "Figure"))
            .count();
        assert!(
            body_groups <= 1,
            "narrow-gutter detector wrongly column-split a single-column body: \
             body spans landed in {} groups",
            body_groups
        );
    }

    /// xycut mirror: a short-verse two-column body —
    /// short tokens per column-line (`mean_chars <= 20`) but a strong
    /// balanced central gutter — must classify as `Prose` (so it gets
    /// cut) and be accepted by `detect_narrow_gutter_prose`, even though
    /// the long-line `mean_chars > 20` guard would reject it.
    #[test]
    fn test_short_verse_two_column_classified_prose_and_cut() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        // Two columns: left starts at x=40, right at x=240. Each verse
        // line carries two short, EQUAL-length 4-char tokens per side
        // (8 non-whitespace chars/side → 16 chars/line, so mean_chars
        // ≤ 20 and the long-line prose guard does NOT apply — this
        // exercises the new short-line admission arm). The left column's
        // right edge lands consistently near x≈94 and the gutter gap
        // midpoint is stable at ≈167 every line (region x≈40..≈294,
        // width≈254, gutter offset ≈0.50·width). Stable gap → high
        // corridor concentration; equal token counts → balanced char
        // mass; two tight left-column start X's (40, 72) within one
        // column → ≤ 2 left-edge clusters. Uniform token widths keep the
        // within-line gap midpoint inside the 10 pt clustering radius. ~keep
        let left_lines = [
            ["comm", "lalu"],
            ["crea", "ciel"],
            ["terr", "etai"],
            ["info", "vide"],
            ["surf", "labi"],
            ["espr", "leau"],
        ];
        let right_lines = [
            ["EtD1", "ditq"],
            ["lumi", "soit"],
            ["etla", "fut1"],
            ["Dieu", "vitq"],
            ["bonn", "ilse"],
            ["aral", "obsc"],
        ];
        let mut spans = Vec::new();
        // 24 lines total (4 verse-stanzas of 6) so the body clears the
        // ≥12 gap-bearing-line floor in detect_narrow_gutter_prose. ~keep
        for rep in 0..4 {
            for i in 0..6 {
                let y = 600.0 - ((rep * 6 + i) as f32) * 14.0;
                spans.push(make_word(40.0, y, left_lines[i][0]));
                spans.push(make_word(72.0, y, left_lines[i][1]));
                spans.push(make_word(240.0, y, right_lines[i][0]));
                spans.push(make_word(272.0, y, right_lines[i][1]));
            }
        }

        let indices: Vec<usize> = (0..spans.len()).collect();
        assert_eq!(
            strategy.classify_region_kind(&spans, &indices),
            RegionKind::Prose,
            "short-verse two-column body with a strong balanced central \
             corridor must classify as Prose despite mean_chars <= 20"
        );
        assert!(
            strategy
                .detect_narrow_gutter_prose(&spans, &indices, strategy.classify_region_kind(&spans, &indices),)
                .is_some(),
            "detect_narrow_gutter_prose must accept the routed short-verse \
             body (gutter found) so it is cut at the gutter"
        );
    }

    /// xycut mirror — negative: a short-cell multi-column
    /// numeric table (four narrow digit columns → short cells with ≥ 3
    /// left-edge clusters and scattered within-line gaps) must STILL
    /// classify as `Table` and NOT be accepted for cutting.
    #[test]
    fn test_short_cell_label_table_still_table_not_cut() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        // A lopsided label+data table: a tiny numeric label column at
        // x=40 (a single digit, ~1 char) and a wide data column at x=100
        // (~8 chars). The within-line gutter is consistent, so the
        // corridor concentration/coverage/centre guards alone would NOT
        // reject it — but the left/right non-whitespace char balance is
        // grossly lopsided (label side ≈ 11 % of chars, well under the
        // 35 % floor), the length-independent table discriminator. A
        // genuine two-column verse body has balanced sides; this table
        // does not, so the short-line admission must reject it.
        // mean_chars ≈ 9 (≥ 8), so it does NOT fall through the
        // `mean_chars < 8 → Table` branch either — the balance check is
        // what keeps it out of Prose. ~keep
        let mut spans = Vec::new();
        let labels = ["7", "8", "9", "5", "3", "1"];
        let data = ["12345678", "23456781", "34567812", "45678123"];
        for i in 0..24 {
            let y = 600.0 - (i as f32) * 14.0;
            spans.push(make_word(40.0, y, labels[i % labels.len()]));
            spans.push(make_word(100.0, y, data[i % data.len()]));
        }

        let indices: Vec<usize> = (0..spans.len()).collect();
        assert_ne!(
            strategy.classify_region_kind(&spans, &indices),
            RegionKind::Prose,
            "lopsided label+data table must NOT be admitted as Prose \
             (left/right char mass is unbalanced — label column is tiny)"
        );
        assert!(
            strategy
                .detect_narrow_gutter_prose(&spans, &indices, strategy.classify_region_kind(&spans, &indices),)
                .is_none(),
            "detect_narrow_gutter_prose must reject the short-cell table \
             (no central-corridor Prose admission) so it is NOT cut"
        );
    }

    #[test]
    fn test_degenerate_ctm_partition_region_does_not_abort() {
        let strategy = XYCutStrategy::new();
        let degenerate_x: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(10.0, 85.0, 30.0, 10.0),
            make_span(10.0, 70.0, 30.0, 10.0),
            make_span(10.0, 55.0, 30.0, 10.0),
            make_span(10.0, 40.0, 30.0, 10.0),
            make_span(degenerate_x, 100.0, 30.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, spans.len(), "all spans must be preserved");
    }

    /// Many distinct-Y single spans is the singleton-peel pathology. With the
    /// depth cap, `partition_region` must still terminate and preserve every
    /// span (the cap falls back to a flat sort, which keeps all indices).
    #[test]
    fn test_partition_indexed_depth_guard_preserves_all_spans() {
        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        // 300 spans, each on its own Y band — deeper than MAX_PARTITION_DEPTH. ~keep
        let spans: Vec<TextSpan> = (0..300)
            .map(|i| make_span(10.0, (i as f32) * 11.0, 30.0, 10.0))
            .collect();

        let groups = strategy.partition_region(&spans);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, spans.len(), "depth guard must not drop spans");
    }
}
