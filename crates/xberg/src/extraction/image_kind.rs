//! Image classification and clustering for document extraction.
//!
//! This module provides heuristic classification of extracted images and
//! spatial clustering to identify raster tile fragments that compose a single figure.

use crate::types::{ExtractedImage, ImageKind};
use std::collections::HashMap;

/// Pixel-area below which the small-image rules (Icon / Decoration) trigger.
/// Tuned for typical icon sizes (16×16 to 64×64).
const SMALL_IMAGE_AREA: u64 = 64 * 64;
/// Aspect-ratio band that distinguishes a near-square Icon from a Decoration strip.
const ICON_ASPECT_LOW: f64 = 0.8;
const ICON_ASPECT_HIGH: f64 = 1.2;
/// A Decoration is a tiny image with an extreme aspect ratio outside this band.
const DECORATION_ASPECT_LOW: f64 = 0.2;
const DECORATION_ASPECT_HIGH: f64 = 5.0;
/// Pixel-area above which a JPEG is biased toward Photograph.
const LARGE_JPEG_AREA: u64 = 800 * 800;
/// Pixel-area below which low-entropy images are classified as Chart rather than
/// Photograph (charts tend to be small, palette-poor compared to photos).
const SMALL_CHART_AREA: u64 = 400 * 400;
/// Shannon-entropy threshold (bits / byte) that biases an image toward Photograph.
const HIGH_ENTROPY_THRESHOLD: f64 = 6.0;
/// Shannon-entropy threshold below which a small image is classified as Chart.
const LOW_ENTROPY_THRESHOLD: f64 = 3.0;
/// Hard cap on the source-image pixel count we are willing to fully decode for
/// entropy analysis. Beyond this we skip the entropy step rather than risk a
/// multi-gigabyte decoded allocation.
const MAX_CLASSIFY_PIXELS: u64 = 64 * 1024 * 1024;

/// Classify an image based on its metadata and visual properties.
///
/// Uses a rule cascade over already-captured signals: dimensions, aspect ratio,
/// colorspace, bits-per-component, format, and histogram entropy on a downsampled
/// 64×64 thumbnail.
///
/// # Arguments
///
/// * `bytes` — Raw image bytes (should be decodable to standard formats)
/// * `format` — Image format (e.g., "jpeg", "png", "ccitt")
/// * `width` — Image width in pixels
/// * `height` — Image height in pixels
/// * `colorspace` — Colorspace name (e.g., "RGB", "CMYK", "Gray", "Indexed")
/// * `bits_per_component` — Bits per color component (e.g., 1, 8, 16)
/// * `is_mask` — Whether this image is a transparency or alpha mask
///
/// # Returns
///
/// A tuple of `(ImageKind, confidence)` where confidence is in [0.0, 1.0].
/// Returns `(Unknown, 0.0)` if bytes cannot be decoded.
#[cfg_attr(alef, alef(skip))]
pub fn classify(
    bytes: &[u8],
    format: &str,
    width: Option<u32>,
    height: Option<u32>,
    colorspace: Option<&str>,
    bits_per_component: Option<u32>,
    is_mask: bool,
) -> (ImageKind, f32) {
    if is_mask {
        return (ImageKind::Mask, 0.95);
    }

    let w = width.unwrap_or(1);
    let h = height.unwrap_or(1);
    let area = (w as u64) * (h as u64);
    let aspect = if h > 0 { (w as f64) / (h as f64) } else { 1.0 };

    if w == 0 || h == 0 {
        return (ImageKind::Unknown, 0.0);
    }
    if let Some(result) = classify_small_image(area, aspect) {
        return result;
    }
    if let Some(result) = classify_by_colorspace_and_format(format, area, colorspace, bits_per_component) {
        return result;
    }
    if let Some(result) = classify_by_entropy(bytes, w, h, area) {
        return result;
    }

    (ImageKind::Unknown, 0.50)
}

/// Icon/Decoration rules for small images: a near-square small image is an Icon, and any other
/// small image with an extreme aspect ratio is a Decoration.
fn classify_small_image(area: u64, aspect: f64) -> Option<(ImageKind, f32)> {
    if area >= SMALL_IMAGE_AREA {
        return None;
    }
    if aspect > ICON_ASPECT_LOW && aspect < ICON_ASPECT_HIGH {
        return Some((ImageKind::Icon, 0.85));
    }
    if !(ICON_ASPECT_LOW..=ICON_ASPECT_HIGH).contains(&aspect) {
        let confidence = if (DECORATION_ASPECT_LOW..=DECORATION_ASPECT_HIGH).contains(&aspect) {
            0.65
        } else {
            0.80
        };
        return Some((ImageKind::Decoration, confidence));
    }
    None
}

/// Colorspace/format rules: Gray 1-bit reads as a scanned text block, CMYK 8-bit as a
/// photograph, a large JPEG as a photograph, indexed-color Flate as a diagram, and CCITT
/// (fax/bilevel) as a mask.
fn classify_by_colorspace_and_format(
    format: &str,
    area: u64,
    colorspace: Option<&str>,
    bits_per_component: Option<u32>,
) -> Option<(ImageKind, f32)> {
    if colorspace == Some("Gray") && bits_per_component == Some(1) {
        return Some((ImageKind::TextBlock, 0.75));
    }
    if colorspace == Some("CMYK") && bits_per_component == Some(8) {
        return Some((ImageKind::Photograph, 0.70));
    }
    if format == "jpeg" && area > LARGE_JPEG_AREA {
        return Some((ImageKind::Photograph, 0.85));
    }
    if format == "flate" && colorspace == Some("Indexed") {
        return Some((ImageKind::Diagram, 0.65));
    }
    if format == "ccitt" {
        return Some((ImageKind::Mask, 0.85));
    }
    None
}

/// Entropy-based fallback: a high-entropy thumbnail reads as a photograph, and a small,
/// low-entropy one as a chart. Skips images too large to safely decode for the thumbnail.
fn classify_by_entropy(bytes: &[u8], width: u32, height: u32, area: u64) -> Option<(ImageKind, f32)> {
    if area == 0 || area > MAX_CLASSIFY_PIXELS {
        return None;
    }
    let entropy = compute_entropy_on_thumbnail(bytes, width, height).ok()?;
    if entropy > HIGH_ENTROPY_THRESHOLD {
        return Some((ImageKind::Photograph, 0.65));
    }
    if entropy < LOW_ENTROPY_THRESHOLD && area < SMALL_CHART_AREA {
        return Some((ImageKind::Chart, 0.60));
    }
    None
}

/// Iterative path-compressing find for the cluster_tiles union-find.
///
/// Two passes: first walk to the root, then re-walk and rewrite each parent
/// pointer to that root. Iterative to avoid stack overflow on adversarial
/// inputs (a chain of N parent pointers would otherwise consume N stack frames).
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    let mut root = x;
    while parent[root] != root {
        root = parent[root];
    }
    while parent[x] != root {
        let next = parent[x];
        parent[x] = root;
        x = next;
    }
    root
}

/// Union two nodes in the union-find, rooting at the smaller index for
/// determinism (so cluster IDs follow document reading order).
fn uf_union(parent: &mut [usize], x: usize, y: usize) {
    let px = uf_find(parent, x);
    let py = uf_find(parent, y);
    if px != py {
        let (smaller, larger) = if px < py { (px, py) } else { (py, px) };
        parent[larger] = smaller;
    }
}

/// Compute entropy of a downsampled 64×64 thumbnail.
///
/// Attempts to load the image using the `image` crate, resize to 64×64,
/// and compute Shannon entropy of the flattened RGB histogram.
/// Returns `Err` if the image cannot be decoded or is too small.
///
/// Only available when the `image-processing` feature is enabled (via ocr or ocr-wasm).
#[cfg(any(feature = "ocr", feature = "ocr-wasm"))]
fn compute_entropy_on_thumbnail(bytes: &[u8], _width: u32, _height: u32) -> Result<f64, String> {
    use image::imageops::FilterType;

    const THUMBNAIL_RGB_BYTES: u64 = 64 * 64 * 3;
    let rgb =
        crate::extraction::image_decode::decode_standard_rgb8_with_additional_live_bytes_and_default_security_limits(
            bytes,
            THUMBNAIL_RGB_BYTES,
        )
        .map_err(|error| error.to_string())?;
    let img = image::DynamicImage::ImageRgb8(rgb);

    let thumb = img.resize_exact(64, 64, FilterType::Lanczos3);

    let rgb = thumb.into_rgb8();
    let pixels = rgb.as_raw();

    let mut histogram = vec![0u32; 256];
    for &byte in pixels {
        histogram[byte as usize] += 1;
    }

    let total = pixels.len() as f64;
    let mut entropy = 0.0;
    for count in histogram {
        if count > 0 {
            let p = count as f64 / total;
            entropy -= p * p.log2();
        }
    }

    Ok(entropy)
}

/// Fallback entropy computation when image crate is unavailable.
#[cfg(not(any(feature = "ocr", feature = "ocr-wasm")))]
fn compute_entropy_on_thumbnail(_bytes: &[u8], _width: u32, _height: u32) -> Result<f64, String> {
    Err("Image processing not available".to_string())
}

/// Cluster spatially adjacent, similarly-sized images on a page.
///
/// Groups images that appear to be tiles of a single figure (e.g., a technical
/// drawing composed of dozens of raster fragments). For each group with 2+ members,
/// assigns a shared `cluster_id` and reclassifies members as `TileFragment`.
///
/// Clustering criteria:
/// - Images must be on the same page
/// - Images must be classified as `Drawing`, `Diagram`, or `TileFragment` (or unclassified with area < 300×300)
/// - Bounding boxes (if present) must be spatially adjacent: within half a tile-side
///   (`min(width, height) / 2`) of each other
/// - Dimensions must match within ±20%
/// - Emits one `info!` span per page with cluster count and max cluster size
#[cfg_attr(alef, alef(skip))]
pub fn cluster_tiles(images: &mut [ExtractedImage]) {
    if images.is_empty() {
        return;
    }

    let mut by_page: HashMap<Option<u32>, Vec<usize>> = HashMap::new();
    for (idx, img) in images.iter().enumerate() {
        by_page.entry(img.page_number).or_default().push(idx);
    }

    let mut next_cluster_id = 1u32;

    for (page_num, indices) in by_page {
        if indices.len() < 2 {
            continue;
        }

        let candidates = select_tile_candidates(images, &indices);
        if candidates.len() < 2 {
            continue;
        }

        let Some(candidates) = narrow_by_median_dimensions(images, &candidates) else {
            continue;
        };

        let parent = union_find_adjacent_tiles(images, &candidates);
        let (cluster_count, max_cluster_size) = apply_tile_clusters(images, &candidates, parent, &mut next_cluster_id);

        if cluster_count > 0 {
            tracing::info!(
                target: "xberg::image_kind",
                page = ?page_num,
                cluster_count,
                max_cluster_size,
                "clustered tile fragments"
            );
        }
    }
}

/// Select candidate indices on a page for tile clustering: images already classified as
/// Drawing/Diagram/TileFragment, plus small unclassified images that might be an unlabeled tile.
fn select_tile_candidates(images: &[ExtractedImage], indices: &[usize]) -> Vec<usize> {
    indices
        .iter()
        .copied()
        .filter(|&idx| {
            let img = &images[idx];
            let is_drawable = matches!(
                img.image_kind,
                Some(ImageKind::Drawing | ImageKind::Diagram | ImageKind::TileFragment)
            );
            let is_unclassified_small = img.image_kind.is_none()
                && (img.width.unwrap_or(0) as u64) * (img.height.unwrap_or(0) as u64) < (300 * 300);
            is_drawable || is_unclassified_small
        })
        .collect()
}

/// Narrow `candidates` to those within ±20% of the group's median width/height, the size
/// signature of tiles composing one figure. Returns `None` when the median is degenerate
/// (< 1px, meaning "not a tile cluster") or fewer than 2 candidates remain after narrowing.
fn narrow_by_median_dimensions(images: &[ExtractedImage], candidates: &[usize]) -> Option<Vec<usize>> {
    let dims: Vec<_> = candidates
        .iter()
        .map(|&idx| {
            let img = &images[idx];
            (img.width.unwrap_or(0), img.height.unwrap_or(0))
        })
        .collect();

    let mut widths: Vec<_> = dims.iter().map(|(w, _)| *w).collect();
    let mut heights: Vec<_> = dims.iter().map(|(_, h)| *h).collect();
    widths.sort();
    heights.sort();

    let median_w = widths[widths.len() / 2] as f64;
    let median_h = heights[heights.len() / 2] as f64;
    if median_w < 1.0 || median_h < 1.0 {
        return None;
    }

    let filtered: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|&idx| {
            let img = &images[idx];
            let w = img.width.unwrap_or(0) as f64;
            let h = img.height.unwrap_or(0) as f64;
            let w_ratio = w / median_w;
            let h_ratio = h / median_h;
            (0.8..=1.2).contains(&w_ratio) && (0.8..=1.2).contains(&h_ratio)
        })
        .collect();

    if filtered.len() < 2 {
        return None;
    }
    Some(filtered)
}

/// Whether two candidate images are spatially adjacent enough to belong to the same tile
/// cluster: within half a tile-side of each other when bounding boxes are known, or within a
/// small index window when they are not.
fn should_connect_tiles(img_i: &ExtractedImage, img_j: &ExtractedImage, idx_i: usize, idx_j: usize) -> bool {
    if let (Some(bbox_i), Some(bbox_j)) = (&img_i.bounding_box, &img_j.bounding_box) {
        let min_dim = (img_i.width.unwrap_or(0) as i32)
            .min(img_i.height.unwrap_or(0) as i32)
            .min(img_j.width.unwrap_or(0) as i32)
            .min(img_j.height.unwrap_or(0) as i32) as f64;

        if min_dim < 1.0 {
            return false;
        }
        let threshold = min_dim / 2.0;
        let dx = (bbox_i.x0.max(bbox_j.x0) - bbox_i.x1.min(bbox_j.x1)).max(0.0);
        let dy = (bbox_i.y0.max(bbox_j.y0) - bbox_i.y1.min(bbox_j.y1)).max(0.0);
        let dist = (dx * dx + dy * dy).sqrt();
        dist <= threshold
    } else {
        const NO_BBOX_INDEX_WINDOW: i32 = 3;
        (idx_i as i32 - idx_j as i32).abs() <= NO_BBOX_INDEX_WINDOW
    }
}

/// Union-find adjacent candidates (pairwise, via [`should_connect_tiles`]) and return the
/// resulting parent array, indexed by position within `candidates` (not by `images` index).
fn union_find_adjacent_tiles(images: &[ExtractedImage], candidates: &[usize]) -> Vec<usize> {
    let n = candidates.len();
    let mut parent: Vec<usize> = (0..n).collect();

    for (i, idx_i) in candidates.iter().enumerate() {
        for (j, idx_j) in candidates.iter().enumerate().skip(i + 1) {
            let img_i = &images[*idx_i];
            let img_j = &images[*idx_j];
            if should_connect_tiles(img_i, img_j, *idx_i, *idx_j) {
                uf_union(&mut parent, i, j);
            }
        }
    }

    parent
}

/// Group `candidates` into clusters by `parent`'s union-find roots, assign each multi-member
/// cluster a shared `cluster_id`, reclassify Drawing/Diagram members as TileFragment, and return
/// `(cluster_count, max_cluster_size)` for the caller's tracing span.
fn apply_tile_clusters(
    images: &mut [ExtractedImage],
    candidates: &[usize],
    mut parent: Vec<usize>,
    next_cluster_id: &mut u32,
) -> (i32, usize) {
    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, idx_i) in candidates.iter().enumerate() {
        let root = uf_find(&mut parent, i);
        clusters.entry(root).or_default().push(*idx_i);
    }

    let mut cluster_count = 0;
    let mut max_cluster_size = 0;
    let mut multi_clusters: Vec<Vec<usize>> = clusters.into_values().filter(|cluster| cluster.len() >= 2).collect();
    for cluster in &mut multi_clusters {
        cluster.sort_unstable();
    }
    multi_clusters.sort_by_key(|cluster| cluster[0]);
    for cluster in multi_clusters {
        cluster_count += 1;
        max_cluster_size = max_cluster_size.max(cluster.len());
        for idx in cluster {
            images[idx].cluster_id = Some(*next_cluster_id);
            if matches!(images[idx].image_kind, Some(ImageKind::Drawing | ImageKind::Diagram)) {
                images[idx].image_kind = Some(ImageKind::TileFragment);
            }
        }
        *next_cluster_id = next_cluster_id.saturating_add(1);
    }

    (cluster_count, max_cluster_size)
}

#[cfg(test)]
#[path = "image_kind/tests.rs"]
mod tests;
