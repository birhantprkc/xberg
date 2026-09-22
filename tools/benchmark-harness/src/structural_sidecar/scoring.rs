//! The SF1 structural metric: [`score_structural`] scores a predicted [`super::StructuralSidecar`]
//! against ground truth across seven dimensions (paragraphs, headings, lists, table topology,
//! table content, binding edges, reading order) and rolls them up with the weights and folding
//! documented on `structural_sidecar` itself.
//!
//! Split out of `structural_sidecar.rs` purely to keep that file under the repo's file-length
//! limit; `use super::*` pulls in the sidecar types (`StructuralSidecar`, `StructuralNode`,
//! `TableNode`, `Cell`) defined there. `score_structural`, `score_markdown`, `StructuralScore`,
//! and `diagnostic_matches` are re-exported from `structural_sidecar` so external callers'
//! paths are unchanged.

use super::*;

const WEIGHT_HEADING: f64 = 2.0;
const WEIGHT_TABLE: f64 = 1.5;
const WEIGHT_LIST: f64 = 1.0;
const WEIGHT_PARAGRAPH: f64 = 0.5;
const WEIGHT_EDGES: f64 = 0.5;
/// Table cell-content quality (D6, GriTS-Con). Weighted equally to table topology
/// (`WEIGHT_TABLE`) so a table's total SF1 influence is topology + content = 3.0 —
/// getting the grid right but the cell text wrong now costs as much as a broken grid.
const WEIGHT_TABLE_CONTENT: f64 = 1.5;

/// Per-heading-level partial credit: score drops by this per level of distance.
const HEADING_LEVEL_STEP: f64 = 0.25;
/// Per-list-depth partial credit: score drops by this per level of depth distance.
const LIST_DEPTH_STEP: f64 = 0.34;
/// Weight split between the two structural sub-scores of headings and lists.
const STRUCT_SPLIT: f64 = 0.5;
/// Per-grid-dimension credit when only one of {rowspan, colspan} agrees.
const SPAN_HALF_CREDIT: f64 = 0.5;

/// Precision/recall/F1 split for one structural dimension. Report-only: it does
/// not feed the SF1 rollup, but lets a low dimension F1 be read as fabrication
/// (low precision) vs omission (low recall) — the crux of the #36 table work.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct DimBreakdown {
    pub f1: f64,
    pub precision: f64,
    pub recall: f64,
}

/// The scored structural dimensions and the rolled-up SF1. D0–D4 and D6 feed the
/// weighted rollup (D6 gated on table presence), which is then order-folded by D5.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StructuralScore {
    /// D0 — paragraph/content F1.
    pub d0_paragraph: f64,
    /// D1 — heading hierarchy.
    pub d1_heading: f64,
    /// D2 — list nesting.
    pub d2_list: f64,
    /// D3 — table topology (GriTS-like).
    pub d3_table: f64,
    /// D4 — caption/footnote binding-edge F1.
    pub d4_edges: f64,
    /// D5 — reading order (LIS).
    pub d5_order: f64,
    /// D6 — table cell-content F1 (GriTS-Con, position-independent). Report-only:
    /// it does NOT feed the SF1 rollup yet (see Phase 2). Defaults to 1.0 for
    /// scores deserialized from data written before this dimension existed.
    #[serde(default = "one")]
    pub d6_table_content: f64,
    /// Weighted, order-folded SF1 rollup.
    pub sf1: f64,
    /// Per-dimension precision/recall split, parallel to [`Self::dimensions`].
    /// Report-only diagnostics; absent on scores deserialized from older data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<DimBreakdown>,
}

/// Serde default for [`StructuralScore::d6_table_content`] on legacy data.
fn one() -> f64 {
    1.0
}

impl StructuralScore {
    /// Named dimension scores used by benchmark comparison reports. `order` and
    /// `table_content` are reported here but scored/folded separately from the
    /// weighted rollup.
    pub fn dimensions(&self) -> [(&'static str, f64); 7] {
        [
            ("paragraph", self.d0_paragraph),
            ("heading", self.d1_heading),
            ("list", self.d2_list),
            ("table", self.d3_table),
            ("edges", self.d4_edges),
            ("order", self.d5_order),
            ("table_content", self.d6_table_content),
        ]
    }

    /// Named dimensions paired with their precision/recall breakdown. Empty
    /// breakdowns (older data) fall back to a zeroed split carrying the F1.
    pub fn dimensions_pr(&self) -> [(&'static str, DimBreakdown); 7] {
        let dims = self.dimensions();
        std::array::from_fn(|i| {
            let (name, f1) = dims[i];
            let bd = self.breakdown.get(i).copied().unwrap_or(DimBreakdown {
                f1,
                precision: f1,
                recall: f1,
            });
            (name, bd)
        })
    }
}

fn content_sim(a: &str, b: &str) -> f64 {
    compute_f1(&tokenize(a), &tokenize(b))
}

/// Greedy highest-score-first bipartite match over content texts.
/// Returns `(pred_idx, gt_idx, sim)` for each matched pair.
fn greedy_match(pred: &[String], gt: &[String]) -> Vec<(usize, usize, f64)> {
    let mut cands: Vec<(usize, usize, f64)> = Vec::new();
    for (i, p) in pred.iter().enumerate() {
        for (j, g) in gt.iter().enumerate() {
            let s = content_sim(p, g);
            if s > 0.0 {
                cands.push((i, j, s));
            }
        }
    }
    cands.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut used_pred = vec![false; pred.len()];
    let mut used_gt = vec![false; gt.len()];
    let mut out = Vec::new();
    for (i, j, s) in cands {
        if used_pred[i] || used_gt[j] {
            continue;
        }
        used_pred[i] = true;
        used_gt[j] = true;
        out.push((i, j, s));
    }
    out
}

/// Precision, recall, and F1 from a sum of matched credit against pred and gt
/// cardinalities. Precision reads as "how much of what we emitted was right"
/// (over-fabrication when low), recall as "how much of the truth we recovered".
fn f1_parts_from(matched_credit: f64, n_pred: usize, n_gt: usize) -> (f64, f64, f64) {
    if n_pred == 0 && n_gt == 0 {
        return (1.0, 1.0, 1.0);
    }
    if n_pred == 0 || n_gt == 0 {
        return (0.0, 0.0, 0.0);
    }
    let precision = matched_credit / n_pred as f64;
    let recall = matched_credit / n_gt as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (f1, precision, recall)
}

/// F1 from a sum of matched credit against pred and gt cardinalities.
fn f1_from(matched_credit: f64, n_pred: usize, n_gt: usize) -> f64 {
    f1_parts_from(matched_credit, n_pred, n_gt).0
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union > 0.0 { inter / union } else { 1.0 }
}

/// Score the six structural dimensions of `pred` against `gt` and roll them up
/// into SF1.
pub fn score_structural(pred: &StructuralSidecar, gt: &StructuralSidecar) -> StructuralScore {
    let d0 = score_paragraphs(pred, gt);
    let d1 = score_headings(pred, gt);
    let d2 = score_lists(pred, gt);
    let d3 = score_tables(pred, gt);
    let d4 = score_edges(pred, gt);
    let (d5, matched) = score_order(pred, gt);
    let d6 = score_tables_content(pred, gt);

    let mut weight_sum = 0.0;
    let mut score_sum = 0.0;
    let dims = [
        (present_paragraphs(pred, gt), WEIGHT_PARAGRAPH, d0.value),
        (present_headings(pred, gt), WEIGHT_HEADING, d1.value),
        (present_lists(pred, gt), WEIGHT_LIST, d2.value),
        (present_tables(pred, gt), WEIGHT_TABLE, d3.value),
        (present_edges(pred, gt), WEIGHT_EDGES, d4.value),
        // D6 cell-content, gated on the same `present_tables` as topology so
        // table-less docs are unaffected. Folded additively into `base`. ~keep
        (present_tables(pred, gt), WEIGHT_TABLE_CONTENT, d6.value),
    ];
    for (present, weight, value) in dims {
        if present {
            weight_sum += weight;
            score_sum += weight * value;
        }
    }
    // paragraph/heading/list/table/edge dimension is "present" per the `present_*`
    // gates above. That happens either because both documents are genuinely empty
    // of structural content (a true vacuous match, scored 1.0 — mirrors
    // `compute_f1`'s both-empty convention in quality.rs) OR because content exists
    // (e.g. unbound captions/footnotes, which aren't gradeable by any `present_*`
    // gate) on at least one side without a matching gate. The latter must NOT score
    // 1.0: that would let a framework mangle un-gated content for free. Only the
    // former — both node lists literally empty — is a genuine vacuous match.
    let base = if weight_sum > 0.0 {
        score_sum / weight_sum
    } else if pred.nodes.is_empty() && gt.nodes.is_empty() {
        1.0
    } else {
        0.0
    };
    let sf1 = fold_order_into_sf1(base, d5, matched);

    let breakdown = vec![
        d0.breakdown(),
        d1.breakdown(),
        d2.breakdown(),
        d3.breakdown(),
        d4.breakdown(),
        DimBreakdown {
            f1: d5,
            precision: d5,
            recall: d5,
        },
        d6.breakdown(),
    ];

    StructuralScore {
        d0_paragraph: d0.value,
        d1_heading: d1.value,
        d2_list: d2.value,
        d3_table: d3.value,
        d4_edges: d4.value,
        d5_order: d5,
        d6_table_content: d6.value,
        sf1,
        breakdown,
    }
}

/// Parse two Markdown documents and compute canonical SF1.
pub fn score_markdown(predicted: &str, ground_truth: &str) -> StructuralScore {
    score_structural(
        &StructuralSidecar::from_markdown(predicted),
        &StructuralSidecar::from_markdown(ground_truth),
    )
}

/// Content-based node matches used only to explain a canonical SF1 score.
pub(crate) fn diagnostic_matches(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Vec<(usize, usize, f64)> {
    let pred_text: Vec<String> = pred.nodes.iter().map(StructuralNode::repr_text).collect();
    let gt_text: Vec<String> = gt.nodes.iter().map(StructuralNode::repr_text).collect();
    greedy_match(&pred_text, &gt_text)
}

/// A dimension score: the F1 `value` that feeds the SF1 rollup, plus the
/// precision/recall split kept for report-only diagnostics.
struct Dim {
    value: f64,
    precision: f64,
    recall: f64,
}

impl Dim {
    /// Build a dimension score from matched credit against pred/gt cardinalities.
    fn from_credit(matched_credit: f64, n_pred: usize, n_gt: usize) -> Self {
        let (value, precision, recall) = f1_parts_from(matched_credit, n_pred, n_gt);
        Self {
            value,
            precision,
            recall,
        }
    }

    /// A dimension both sides agree is absent (or a perfect match): all 1.0.
    fn perfect() -> Self {
        Self {
            value: 1.0,
            precision: 1.0,
            recall: 1.0,
        }
    }

    /// A dimension present on exactly one side (fabricated or dropped): all 0.0.
    fn zero() -> Self {
        Self {
            value: 0.0,
            precision: 0.0,
            recall: 0.0,
        }
    }

    /// The report-only precision/recall/F1 view of this dimension.
    fn breakdown(&self) -> DimBreakdown {
        DimBreakdown {
            f1: self.value,
            precision: self.precision,
            recall: self.recall,
        }
    }
}

fn paragraph_texts(s: &StructuralSidecar) -> Vec<String> {
    s.nodes
        .iter()
        .filter_map(|n| match n {
            StructuralNode::Paragraph { text } | StructuralNode::Formula { text, .. } => Some(text.clone()),
            StructuralNode::Image { alt } => Some(alt.clone()),
            StructuralNode::Figure { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn present_paragraphs(p: &StructuralSidecar, g: &StructuralSidecar) -> bool {
    !paragraph_texts(p).is_empty() || !paragraph_texts(g).is_empty()
}

fn score_paragraphs(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let pp = paragraph_texts(pred);
    let gg = paragraph_texts(gt);
    let credit: f64 = greedy_match(&pp, &gg).iter().map(|(_, _, s)| *s).sum();
    Dim::from_credit(credit, pp.len(), gg.len())
}

struct HeadingInfo {
    text: String,
    level: u8,
    ancestors: HashSet<String>,
}

fn heading_infos(s: &StructuralSidecar) -> Vec<HeadingInfo> {
    s.nodes
        .iter()
        .filter_map(|n| match n {
            StructuralNode::Heading { level, path, text, .. } => Some(HeadingInfo {
                text: text.clone(),
                level: *level,
                ancestors: path.iter().map(|t| t.to_ascii_lowercase()).collect(),
            }),
            _ => None,
        })
        .collect()
}

fn present_headings(p: &StructuralSidecar, g: &StructuralSidecar) -> bool {
    !heading_infos(p).is_empty() || !heading_infos(g).is_empty()
}

fn score_headings(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let ph = heading_infos(pred);
    let gh = heading_infos(gt);
    let ptext: Vec<String> = ph.iter().map(|h| h.text.clone()).collect();
    let gtext: Vec<String> = gh.iter().map(|h| h.text.clone()).collect();
    let credit: f64 = greedy_match(&ptext, &gtext)
        .iter()
        .map(|(i, j, sim)| {
            let a = &ph[*i];
            let b = &gh[*j];
            let level_score = if a.level == b.level {
                1.0
            } else {
                (1.0 - HEADING_LEVEL_STEP * (a.level as i16 - b.level as i16).unsigned_abs() as f64).max(0.0)
            };
            let ancestor_sim = jaccard(&a.ancestors, &b.ancestors);
            sim * (STRUCT_SPLIT * level_score + STRUCT_SPLIT * ancestor_sim)
        })
        .sum();
    Dim::from_credit(credit, ph.len(), gh.len())
}

struct ListInfo {
    text: String,
    depth: usize,
    ordered: bool,
}

fn list_infos(s: &StructuralSidecar) -> Vec<ListInfo> {
    s.nodes
        .iter()
        .filter_map(|n| match n {
            StructuralNode::ListItem {
                depth, ordered, text, ..
            } => Some(ListInfo {
                text: text.clone(),
                depth: *depth,
                ordered: *ordered,
            }),
            _ => None,
        })
        .collect()
}

fn present_lists(p: &StructuralSidecar, g: &StructuralSidecar) -> bool {
    !list_infos(p).is_empty() || !list_infos(g).is_empty()
}

fn score_lists(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let pl = list_infos(pred);
    let gl = list_infos(gt);
    let ptext: Vec<String> = pl.iter().map(|l| l.text.clone()).collect();
    let gtext: Vec<String> = gl.iter().map(|l| l.text.clone()).collect();
    let credit: f64 = greedy_match(&ptext, &gtext)
        .iter()
        .map(|(i, j, sim)| {
            let a = &pl[*i];
            let b = &gl[*j];
            let depth_score = if a.depth == b.depth {
                1.0
            } else {
                (1.0 - LIST_DEPTH_STEP * (a.depth as i64 - b.depth as i64).unsigned_abs() as f64).max(0.0)
            };
            let ordered_score = if a.ordered == b.ordered { 1.0 } else { 0.0 };
            sim * (STRUCT_SPLIT * depth_score + STRUCT_SPLIT * ordered_score)
        })
        .sum();
    Dim::from_credit(credit, pl.len(), gl.len())
}

fn tables(s: &StructuralSidecar) -> Vec<&TableNode> {
    s.nodes
        .iter()
        .filter_map(|n| match n {
            StructuralNode::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn present_tables(p: &StructuralSidecar, g: &StructuralSidecar) -> bool {
    !tables(p).is_empty() || !tables(g).is_empty()
}

/// GriTS-like grid F1 between two tables: cells sharing a `(row, col)` origin
/// contribute `content_sim * span_agreement`.
fn grits(pred: &TableNode, gt: &TableNode) -> f64 {
    let gt_by_pos: HashMap<(usize, usize), &Cell> = gt.cells.iter().map(|c| ((c.row, c.col), c)).collect();
    let mut credit = 0.0;
    for pc in &pred.cells {
        if let Some(gc) = gt_by_pos.get(&(pc.row, pc.col)) {
            let sim = content_sim(&pc.text, &gc.text);
            let row_ok = pc.rowspan == gc.rowspan;
            let col_ok = pc.colspan == gc.colspan;
            let span = match (row_ok, col_ok) {
                (true, true) => 1.0,
                (true, false) | (false, true) => SPAN_HALF_CREDIT,
                (false, false) => 0.0,
            };
            credit += sim * span;
        }
    }
    f1_from(credit, pred.cells.len(), gt.cells.len())
}

fn score_tables(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let pt = tables(pred);
    let gt_tables = tables(gt);
    if pt.is_empty() && gt_tables.is_empty() {
        return Dim::perfect();
    }
    if pt.is_empty() || gt_tables.is_empty() {
        // A fabricated table (pred-only) or a dropped table (gt-only) scores 0. ~keep
        return Dim::zero();
    }
    let ptext: Vec<String> = pt
        .iter()
        .map(|t| t.cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "))
        .collect();
    let gtext: Vec<String> = gt_tables
        .iter()
        .map(|t| t.cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "))
        .collect();
    let credit: f64 = greedy_match(&ptext, &gtext)
        .iter()
        .map(|(i, j, _)| grits(pt[*i], gt_tables[*j]))
        .sum();
    Dim::from_credit(credit, pt.len(), gt_tables.len())
}

/// GriTS-Con: position-independent cell-content F1 between two tables. Unlike
/// [`grits`] (which credits only cells sharing a `(row, col)` origin), this
/// greedily matches cells by text similarity, so it measures whether the right
/// *content* was recovered regardless of where the grid placed it.
fn grits_con(pred: &TableNode, gt: &TableNode) -> f64 {
    let ptext: Vec<String> = pred.cells.iter().map(|c| c.text.clone()).collect();
    let gtext: Vec<String> = gt.cells.iter().map(|c| c.text.clone()).collect();
    let credit: f64 = greedy_match(&ptext, &gtext).iter().map(|(_, _, s)| *s).sum();
    f1_from(credit, pred.cells.len(), gt.cells.len())
}

/// D6 — table cell-content F1 (GriTS-Con). Mirrors [`score_tables`]'s table
/// matching but scores each matched pair on content recovery ([`grits_con`])
/// rather than grid topology. Report-only; not folded into SF1 (see Phase 2).
fn score_tables_content(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let pt = tables(pred);
    let gt_tables = tables(gt);
    if pt.is_empty() && gt_tables.is_empty() {
        return Dim::perfect();
    }
    if pt.is_empty() || gt_tables.is_empty() {
        return Dim::zero();
    }
    let ptext: Vec<String> = pt
        .iter()
        .map(|t| t.cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "))
        .collect();
    let gtext: Vec<String> = gt_tables
        .iter()
        .map(|t| t.cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "))
        .collect();
    let credit: f64 = greedy_match(&ptext, &gtext)
        .iter()
        .map(|(i, j, _)| grits_con(pt[*i], gt_tables[*j]))
        .sum();
    Dim::from_credit(credit, pt.len(), gt_tables.len())
}

struct Edge {
    caption: String,
    target: String,
}

/// Collect caption/footnote binding edges. Every Caption/Footnote node is
/// gradeable — bound ones carry the resolved target text, unbound ones carry an
/// empty target — so a genuine binding failure is a target mismatch rather than
/// an entry silently missing from both `n_pred` and `n_gt`. Previously only
/// bound nodes (`binds_to: Some(_)`) were collected: a caption unbound on the GT
/// side (a real, if imperfect, occurrence of the deterministic nearest-preceding
/// binder) then had no representation in `edges()`/`present_edges()` at all —
/// content that isn't a Paragraph/Formula/Image/Figure (see `paragraph_texts`)
/// either, so it was invisible to every dimension and any pred output for it was
/// free. Counting it here restores it as a recall opportunity.
fn edges(s: &StructuralSidecar) -> Vec<Edge> {
    s.nodes
        .iter()
        .filter_map(|n| match n {
            StructuralNode::Caption { binds_to, text } | StructuralNode::Footnote { binds_to, text } => Some(Edge {
                caption: text.clone(),
                target: binds_to
                    .and_then(|t| s.nodes.get(t))
                    .map(|n| n.repr_text())
                    .unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

fn present_edges(p: &StructuralSidecar, g: &StructuralSidecar) -> bool {
    !edges(p).is_empty() || !edges(g).is_empty()
}

fn score_edges(pred: &StructuralSidecar, gt: &StructuralSidecar) -> Dim {
    let pe = edges(pred);
    let ge = edges(gt);
    // Match edges by caption similarity; credit weights in target agreement too. ~keep
    let ptext: Vec<String> = pe.iter().map(|e| e.caption.clone()).collect();
    let gtext: Vec<String> = ge.iter().map(|e| e.caption.clone()).collect();
    let credit: f64 = greedy_match(&ptext, &gtext)
        .iter()
        .map(|(i, j, sim)| {
            let target_sim = content_sim(&pe[*i].target, &ge[*j].target);
            sim * target_sim
        })
        .sum();
    Dim::from_credit(credit, pe.len(), ge.len())
}

/// D5: match nodes by content — restricted to pairs of the *same kind*, mirroring
/// every other dimension's same-kind greedy matching (score_paragraphs/headings/
/// lists/tables/edges) — then score reading order via LIS over those matches.
/// Without the kind restriction, a Table's flattened cell text can beat a genuine
/// Paragraph/ListItem match on raw text similarity, corrupting the order pairs
/// with cross-kind matches that no other dimension would ever form. Returns
/// `(order_score, matched_pair_count)`.
fn score_order(pred: &StructuralSidecar, gt: &StructuralSidecar) -> (f64, usize) {
    let pred_pos = order_positions(&pred.reading_order, pred.nodes.len());
    let gt_pos = order_positions(&gt.reading_order, gt.nodes.len());

    let mut pred_by_kind: HashMap<&'static str, Vec<usize>> = HashMap::new();
    for (idx, node) in pred.nodes.iter().enumerate() {
        pred_by_kind.entry(node.kind_name()).or_default().push(idx);
    }
    let mut gt_by_kind: HashMap<&'static str, Vec<usize>> = HashMap::new();
    for (idx, node) in gt.nodes.iter().enumerate() {
        gt_by_kind.entry(node.kind_name()).or_default().push(idx);
    }

    let mut order_pairs: Vec<(usize, usize)> = Vec::new();
    for (kind, pred_indices) in &pred_by_kind {
        let Some(gt_indices) = gt_by_kind.get(kind) else {
            continue;
        };
        let ptext: Vec<String> = pred_indices.iter().map(|&i| pred.nodes[i].repr_text()).collect();
        let gtext: Vec<String> = gt_indices.iter().map(|&j| gt.nodes[j].repr_text()).collect();
        for (local_i, local_j, _) in greedy_match(&ptext, &gtext) {
            let pred_idx = pred_indices[local_i];
            let gt_idx = gt_indices[local_j];
            order_pairs.push((gt_pos[gt_idx], pred_pos[pred_idx]));
        }
    }
    (compute_order_score(&order_pairs), order_pairs.len())
}

/// Map node index → its position in `reading_order` (identity fallback).
fn order_positions(reading_order: &[usize], n: usize) -> Vec<usize> {
    let mut pos = vec![0usize; n];
    if reading_order.len() == n {
        for (p, &idx) in reading_order.iter().enumerate() {
            if idx < n {
                pos[idx] = p;
            }
        }
    } else {
        for (i, slot) in pos.iter_mut().enumerate() {
            *slot = i;
        }
    }
    pos
}

// `scoring.rs` is itself attached via an explicit `#[path]` (see `structural_sidecar.rs`), so
// its own submodule does not inherit an implicit `scoring/`-named directory -- this attribute is
// required, not stylistic. ~keep
#[cfg(test)]
#[path = "scoring/tests.rs"]
mod tests;
