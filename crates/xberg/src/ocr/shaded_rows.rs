//! Per-band (per table row) shading normalization for OCR preprocessing (GH#1785).
//!
//! Whole-page Otsu binarization uses one threshold for the entire page. A subtotal or
//! total row rendered as dark text on a light grey fill, or white text on a mid or dark
//! grey fill, does not survive that single threshold and the whole row is dropped. This
//! module finds each shaded horizontal band, works out its own polarity, and stretches
//! its contrast so the fill becomes white and the ink becomes black, leaving the rest of
//! the page untouched. It is a direct, constant-for-constant Rust port of the `flatten`
//! reference script attached to GH#1785 (`bands.py` + `preproc.py`), which the issue
//! measured against a synthetic and a scanned schedule page.
//!
//! This is deliberately **not** a replacement for whole-page binarization: GH#1785's own
//! measurement shows the per-band step regresses mid-fill rows on the clean page (12/12
//! values read by default, 6/12 after normalization), so it must stay an opt-in step
//! (`ImagePreprocessingConfig::normalize_shaded_rows`, default `false`), not a new
//! default behavior.

use xberg_tesseract::{Pix, Result as PixResult};

/// A row counts as "dark" when at least this fraction of its pixels are below
/// [`DARK_ROW_GRAY_THRESHOLD`]. Mirrors `bands.py`'s `find_bands(frac=0.40)`.
const DARK_ROW_MIN_DARK_FRACTION: f64 = 0.40;
/// A pixel counts as dark (part of a white-text-on-dark-fill band) below this value.
/// Mirrors `bands.py`'s `find_bands(dark=110)`.
const DARK_ROW_GRAY_THRESHOLD: u8 = 110;
/// A run of dark rows shorter than `height / this` (floored at
/// [`MIN_BAND_HEIGHT_FLOOR`]) is noise, not a shaded row. Mirrors `bands.py` /
/// `preproc.py`'s `min_h = max(8, h // 400)`.
const MIN_BAND_HEIGHT_DIVISOR: i32 = 400;
/// See [`MIN_BAND_HEIGHT_DIVISOR`].
const MIN_BAND_HEIGHT_FLOOR: i32 = 8;
/// Within a dark band, a column belongs to the band's ink region when more than this
/// fraction of the band's rows are dark at that column. Mirrors `bands.py`'s
/// `(g[y0:y].mean(axis=0) > 0.5)`.
const DARK_BAND_COLUMN_MAJORITY_FRACTION: f64 = 0.5;
/// Padding (pixels) added around a dark band's bounding box before inverting it, so the
/// invert does not leave a hairline edge at the fill boundary. Mirrors `bands.py`'s
/// `invert_bands(pad=2)`.
const DARK_BAND_INVERT_PADDING: i32 = 2;

/// A shaded (non-white, non-rule) band's row-median gray value must exceed this to count
/// as a fill rather than white paper. Mirrors `preproc.py`'s `shaded_bands(lo=60)`.
const SHADED_BAND_LOW_GRAY: f64 = 60.0;
/// A shaded band's row-median gray value must be below this to count as a fill rather
/// than a black rule. Mirrors `preproc.py`'s `shaded_bands(hi=225)`.
const SHADED_BAND_HIGH_GRAY: f64 = 225.0;
/// The row-median used to detect a shaded band is computed over the central
/// `1 - 2/this` fraction of the table width, skipping the outer eighths that can hold
/// page margin or ruled-line artifacts. Mirrors `preproc.py`'s `g[:, w//8:w*7//8]`.
const TABLE_WIDTH_MARGIN_DENOMINATOR: i32 = 8;
/// Two adjacent shaded rows belong to the same band only while their row medians differ
/// by no more than this; a bigger jump means the fill changed. Mirrors `preproc.py`'s
/// `abs(med[y] - med[y-1]) <= 25`.
const SHADED_BAND_FILL_CHANGE_DELTA: f64 = 25.0;

/// When judging a band's polarity, the top and bottom rows (which usually hold the
/// row's ruled border) are excluded. Mirrors `preproc.py`'s `band[4:-4]`.
const BAND_POLARITY_ROW_MARGIN: usize = 4;
/// A pixel counts towards the polarity vote when it differs from the band's background
/// median by more than this. Mirrors `preproc.py`'s `bg +/- 40`.
const BAND_POLARITY_VOTE_DELTA: f32 = 40.0;
/// A band whose ink-to-background contrast (background median minus the 2nd-percentile
/// "ink" value) is below this is left unmodified rather than stretched, to avoid
/// amplifying noise in a band that is not actually a strong fill. Mirrors `preproc.py`'s
/// `if bg - ink < 30: continue`.
const BAND_MINIMUM_CONTRAST: f32 = 30.0;
/// The percentile used to estimate a band's ink (foreground) value, robust to a
/// minority of anti-aliased edge pixels. Mirrors `preproc.py`'s `np.percentile(band, 2)`.
const BAND_INK_PERCENTILE: f64 = 2.0;
/// The output range a normalized band is linearly stretched into: fill -> white,
/// ink -> black. Mirrors `preproc.py`'s `* 255.0`.
const BAND_STRETCH_MAX: f32 = 255.0;

/// Finds runs of dark rows (candidate white-text-on-dark-fill bands) and returns each
/// band's bounding box `(y0, y1, x0, x1)`, `y1`/`x1` exclusive. Mirrors `bands.py::find_bands`.
fn find_dark_bands(gray: &[u8], width: i32, height: i32) -> Vec<(i32, i32, i32, i32)> {
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let min_band_height = (height / MIN_BAND_HEIGHT_DIVISOR).max(MIN_BAND_HEIGHT_FLOOR);
    let row_dark_fraction = |y: i32| -> f64 {
        let start = (y * width) as usize;
        let end = start + width as usize;
        let dark = gray[start..end]
            .iter()
            .filter(|&&value| value < DARK_ROW_GRAY_THRESHOLD)
            .count();
        dark as f64 / width as f64
    };

    let mut bands = Vec::new();
    let mut y = 0;
    while y < height {
        if row_dark_fraction(y) >= DARK_ROW_MIN_DARK_FRACTION {
            let y0 = y;
            while y < height && row_dark_fraction(y) >= DARK_ROW_MIN_DARK_FRACTION {
                y += 1;
            }
            if y - y0 >= min_band_height {
                bands.push(dark_band_columns(gray, width, y0, y));
            }
        } else {
            y += 1;
        }
    }
    bands
}

/// Restricts a dark band's x-range to the columns where most of the band's rows are
/// dark, matching `bands.py`'s per-column majority vote. Falls back to the full width
/// if no column reaches the majority (mirrors `(x0, x1) = (0, w)` when `cols` is empty).
fn dark_band_columns(gray: &[u8], width: i32, y0: i32, y1: i32) -> (i32, i32, i32, i32) {
    let band_height = (y1 - y0) as f64;
    let mut x0 = width;
    let mut x1 = 0;
    for x in 0..width {
        let dark_rows = (y0..y1)
            .filter(|&y| gray[(y * width + x) as usize] < DARK_ROW_GRAY_THRESHOLD)
            .count();
        if dark_rows as f64 / band_height > DARK_BAND_COLUMN_MAJORITY_FRACTION {
            x0 = x0.min(x);
            x1 = x1.max(x + 1);
        }
    }
    if x0 >= x1 { (y0, y1, 0, width) } else { (y0, y1, x0, x1) }
}

/// Inverts every dark band in place (white text on a dark fill becomes black text on
/// white), padded by [`DARK_BAND_INVERT_PADDING`]. Mirrors `bands.py::invert_bands`.
fn invert_dark_bands(gray: &mut [u8], width: i32, height: i32, bands: &[(i32, i32, i32, i32)]) {
    for &(band_y0, band_y1, band_x0, band_x1) in bands {
        let y0 = (band_y0 - DARK_BAND_INVERT_PADDING).max(0);
        let y1 = (band_y1 + DARK_BAND_INVERT_PADDING).min(height);
        let x0 = (band_x0 - DARK_BAND_INVERT_PADDING).max(0);
        let x1 = (band_x1 + DARK_BAND_INVERT_PADDING).min(width);
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * width + x) as usize;
                gray[index] = 255 - gray[index];
            }
        }
    }
}

/// The median of a slice of grayscale samples (linear-interpolated, matching
/// `numpy.median`/`numpy.percentile(50)`).
fn median(values: &[u8]) -> f64 {
    percentile(values, 50.0)
}

/// The `pct`-th percentile of a slice of grayscale samples, linearly interpolated
/// between the two nearest ranks (matching `numpy.percentile`'s default `linear` method).
fn percentile(values: &[u8], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<u8> = values.to_vec();
    sorted.sort_unstable();
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo] as f64
    } else {
        let fraction = rank - lo as f64;
        sorted[lo] as f64 * (1.0 - fraction) + sorted[hi] as f64 * fraction
    }
}

/// The `pct`-th percentile of a slice of `f32` samples produced by band arithmetic
/// (post-invert values can exceed `u8` intermediate ranges only in the padded, unused
/// scratch copy, so `f32` is used for arithmetic precision, not range).
fn percentile_f32(values: &[f32], pct: f64) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let fraction = (rank - lo as f64) as f32;
        sorted[lo] * (1.0 - fraction) + sorted[hi] * fraction
    }
}

/// Finds shaded bands (fills that are neither white paper nor a black rule) by the
/// median gray value of each row's central table-width span. Mirrors
/// `preproc.py::shaded_bands`. Must run *after* [`invert_dark_bands`], exactly as
/// `preproc.py::flatten` calls `shaded_bands(g)` on the already dark-inverted image.
fn find_shaded_bands(gray: &[u8], width: i32, height: i32) -> Vec<(i32, i32)> {
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let margin = width / TABLE_WIDTH_MARGIN_DENOMINATOR;
    let column_start = margin.max(0);
    let column_end = (width - margin).max(column_start);
    let min_band_height = (height / MIN_BAND_HEIGHT_DIVISOR).max(MIN_BAND_HEIGHT_FLOOR) as usize;

    let row_medians: Vec<f64> = (0..height)
        .map(|y| {
            let start = (y * width + column_start) as usize;
            let end = (y * width + column_end) as usize;
            median(&gray[start..end])
        })
        .collect();
    let is_shaded = |y: usize| row_medians[y] > SHADED_BAND_LOW_GRAY && row_medians[y] < SHADED_BAND_HIGH_GRAY;

    let mut bands = Vec::new();
    let mut y = 0usize;
    let height = height as usize;
    while y < height {
        if is_shaded(y) {
            let y0 = y;
            y += 1;
            while y < height
                && is_shaded(y)
                && (row_medians[y] - row_medians[y - 1]).abs() <= SHADED_BAND_FILL_CHANGE_DELTA
            {
                y += 1;
            }
            if y - y0 >= min_band_height {
                bands.push((y0 as i32, y as i32));
            }
        } else {
            y += 1;
        }
    }
    bands
}

/// Normalizes one shaded band in place: decides its polarity from the pixels far above
/// and below the fill's own median (skipping the top/bottom rule rows), then linearly
/// stretches it so the fill becomes white and the ink becomes black. Leaves the band
/// untouched if its ink-to-background contrast is too low to stretch safely. Mirrors
/// the per-band body of `preproc.py::flatten`.
fn normalize_band(gray: &mut [u8], width: i32, y0: i32, y1: i32) {
    let band_height = (y1 - y0) as usize;
    let band_width = width as usize;
    let mut band: Vec<f32> = gray[(y0 * width) as usize..(y1 * width) as usize]
        .iter()
        .map(|&value| value as f32)
        .collect();

    let background = median_f32(&band);

    let core_row_start = BAND_POLARITY_ROW_MARGIN.min(band_height);
    let core_row_end = band_height.saturating_sub(BAND_POLARITY_ROW_MARGIN).max(core_row_start);
    let column_margin = band_width / TABLE_WIDTH_MARGIN_DENOMINATOR as usize;
    let core_column_start = column_margin;
    let core_column_end = band_width.saturating_sub(column_margin).max(core_column_start);

    let mut light_votes: u64 = 0;
    let mut dark_votes: u64 = 0;
    for row in core_row_start..core_row_end {
        for column in core_column_start..core_column_end {
            let value = band[row * band_width + column];
            if value > background + BAND_POLARITY_VOTE_DELTA {
                light_votes += 1;
            } else if value < background - BAND_POLARITY_VOTE_DELTA {
                dark_votes += 1;
            }
        }
    }

    let background = if light_votes > dark_votes {
        for value in &mut band {
            *value = BAND_STRETCH_MAX - *value;
        }
        median_f32(&band)
    } else {
        background
    };

    let ink = percentile_f32(&band, BAND_INK_PERCENTILE);
    if background - ink < BAND_MINIMUM_CONTRAST {
        return;
    }

    for row in 0..band_height {
        for column in 0..band_width {
            let value = band[row * band_width + column];
            let stretched = (value - ink) / (background - ink) * BAND_STRETCH_MAX;
            let index = (y0 as usize + row) * band_width + column;
            gray[index] = stretched.clamp(0.0, BAND_STRETCH_MAX) as u8;
        }
    }
}

fn median_f32(values: &[f32]) -> f32 {
    percentile_f32(values, 50.0)
}

/// Runs the full shaded-row normalization pipeline over a byte-per-pixel grayscale
/// buffer in place: invert dark (white-on-dark) bands, then flatten every shaded band's
/// polarity and contrast so it reads as ordinary dark-on-white text.
pub(crate) fn normalize_shaded_rows_bytes(gray: &mut [u8], width: i32, height: i32) {
    let dark_bands = find_dark_bands(gray, width, height);
    invert_dark_bands(gray, width, height, &dark_bands);
    for (y0, y1) in find_shaded_bands(gray, width, height) {
        normalize_band(gray, width, y0, y1);
    }
}

/// Applies [`normalize_shaded_rows_bytes`] to a grayscale `Pix`, returning a new `Pix`
/// with the same resolution. The input `pix` must already be 8 bpp grayscale (call
/// `Pix::to_grayscale()` first, as `preprocess_pix` does).
pub(crate) fn normalize_shaded_rows(pix: &Pix) -> PixResult<Pix> {
    let width = pix.width();
    let height = pix.height();
    let mut bytes = pix.grayscale_bytes()?;
    normalize_shaded_rows_bytes(&mut bytes, width, height);
    let mut normalized = Pix::from_grayscale_bytes(&bytes, width as u32, height as u32)?;
    if let Ok((xres, yres)) = pix.get_resolution() {
        normalized.set_resolution(xres, yres)?;
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic page: white background, a plain black-on-white row, a
    /// light-fill row (dark text on a light grey band), and a dark-fill row (white text
    /// on a near-black band), each as one solid horizontal band of "text" pixels
    /// centered in its row so the polarity vote has a clear majority.
    fn synthetic_page(width: i32, height: i32) -> Vec<u8> {
        // Text glyphs cover a minority of a row's pixels, not half of it: a solid
        // half-width text block would itself cross `find_dark_bands`' 40% dark-fraction
        // threshold regardless of the row's fill, which is not representative of real
        // text. `TEXT_STRIPE_MODULUS` thins the "ink" columns to a fixed fraction of the
        // text region so each band's row-level darkness matches the fill it stands for.
        const TEXT_STRIPE_MODULUS: i32 = 5;
        let mut gray = vec![255u8; (width * height) as usize];
        let text_columns = (width / 4)..(width * 3 / 4);
        let is_ink_column = |x: i32| text_columns.contains(&x) && x % TEXT_STRIPE_MODULUS == 0;

        // Light-fill band: rows 10..30, fill=200, sparse dark ink=30 (dark text on light fill).
        for y in 10..30 {
            for x in 0..width {
                let index = (y * width + x) as usize;
                gray[index] = if is_ink_column(x) { 30 } else { 200 };
            }
        }
        // Dark-fill band: rows 40..60, fill=40, sparse light ink=230 (white text on dark fill).
        // The fill itself is dark, so most of the row is dark regardless of the sparse ink.
        for y in 40..60 {
            for x in 0..width {
                let index = (y * width + x) as usize;
                gray[index] = if is_ink_column(x) { 230 } else { 40 };
            }
        }
        gray
    }

    /// The first ink (text) column in the light- and dark-fill bands built by
    /// [`synthetic_page`], for pixel-exact assertions.
    fn first_ink_column(width: i32) -> i32 {
        width / 4
    }

    #[test]
    fn should_find_dark_band_for_white_on_dark_fill_row() {
        let width = 200;
        let height = 80;
        let gray = synthetic_page(width, height);

        let bands = find_dark_bands(&gray, width, height);

        assert_eq!(
            bands.len(),
            1,
            "exactly one band must be dark enough to trigger inversion"
        );
        let (y0, y1, _, _) = bands[0];
        assert_eq!(
            (y0, y1),
            (40, 60),
            "the dark-fill band's row range must be found exactly"
        );
    }

    #[test]
    fn should_invert_dark_band_to_dark_text_on_light() {
        let width = 200;
        let height = 80;
        let mut gray = synthetic_page(width, height);
        let bands = find_dark_bands(&gray, width, height);

        invert_dark_bands(&mut gray, width, height, &bands);

        let fill_pixel = gray[(45 * width + 5) as usize];
        let text_pixel = gray[(45 * width + first_ink_column(width)) as usize];
        assert!(
            fill_pixel > 200,
            "inverted dark fill must read as light: got {fill_pixel}"
        );
        assert!(
            text_pixel < 55,
            "inverted white text must read as dark: got {text_pixel}"
        );
    }

    #[test]
    fn should_find_and_flatten_light_fill_band() {
        let width = 200;
        let height = 80;
        let mut gray = synthetic_page(width, height);
        let dark_bands = find_dark_bands(&gray, width, height);
        invert_dark_bands(&mut gray, width, height, &dark_bands);

        let shaded = find_shaded_bands(&gray, width, height);
        assert!(
            shaded.iter().any(|&(y0, y1)| y0 <= 10 && y1 >= 30),
            "the light-fill band must be detected as shaded: {shaded:?}"
        );

        for (y0, y1) in shaded {
            normalize_band(&mut gray, width, y0, y1);
        }

        let fill_pixel = gray[(15 * width + 5) as usize];
        let text_pixel = gray[(15 * width + first_ink_column(width)) as usize];
        assert!(
            fill_pixel > 240,
            "flattened light fill must read as white: got {fill_pixel}"
        );
        assert!(text_pixel < 15, "flattened text must read as black: got {text_pixel}");
    }

    #[test]
    fn should_leave_plain_white_background_untouched() {
        let width = 200;
        let height = 80;
        let mut gray = synthetic_page(width, height);
        let before = gray.clone();

        normalize_shaded_rows_bytes(&mut gray, width, height);

        for y in 0..10 {
            for x in 0..width {
                let index = (y * width + x) as usize;
                assert_eq!(gray[index], before[index], "plain white rows must not be touched");
            }
        }
    }

    #[test]
    fn should_round_trip_through_pix() {
        let width = 40u32;
        let height = 20u32;
        let data: Vec<u8> = (0..(width * height)).map(|i| (i % 256) as u8).collect();

        let pix = Pix::from_grayscale_bytes(&data, width, height).unwrap();
        let bytes = pix.grayscale_bytes().unwrap();

        assert_eq!(bytes, data, "grayscale bytes must round-trip through Pix exactly");
    }

    #[test]
    fn should_normalize_shaded_rows_through_pix_pipeline() {
        let width = 200u32;
        let height = 80u32;
        let gray = synthetic_page(width as i32, height as i32);
        let pix = Pix::from_grayscale_bytes(&gray, width, height).unwrap();

        let normalized = normalize_shaded_rows(&pix).unwrap();

        assert_eq!(normalized.width(), width as i32);
        assert_eq!(normalized.height(), height as i32);
        let bytes = normalized.grayscale_bytes().unwrap();
        let fill_pixel = bytes[(15 * width as i32 + 5) as usize];
        assert!(
            fill_pixel > 240,
            "light fill must read white after the Pix round trip: got {fill_pixel}"
        );
    }
}
