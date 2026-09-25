//! Centralised image re-encoding helper.
//!
//! This module provides [`re_encode`], which converts an [`ExtractedImage`] in place
//! to a caller-selected target format.  It is designed to be called from the extraction
//! pipeline after OCR has run and before post-processors that consume `data` + `format`.

use std::borrow::Cow;
use std::io::Cursor;

use bytes::Bytes;
use image::{DynamicImage, ImageFormat};
use tracing::warn;

use crate::core::config::extraction::ImageOutputFormat;
#[cfg(feature = "svg")]
use crate::core::config::extraction::SvgOptions;
use crate::error::XbergError;
#[cfg(feature = "heic")]
use crate::extraction::image_decode::{
    ImageDecodeBudget, copy_decoded_rows, decoded_byte_count, image_dimension_error,
};
use crate::extraction::image_decode::{
    decode_standard_image_with_format_and_security_limits, decode_standard_image_with_security_limits,
    validate_dynamic_image_additional_live_bytes,
};
use crate::extractors::security::SecurityLimits;
use crate::types::ExtractedImage;

/// Describes why a re-encode attempt was skipped or failed.
///
/// The pipeline converts `Err(EncodeWarning)` into a `ProcessingWarning` and leaves
/// the image bytes untouched — the caller is never left with a partially-written image.
#[derive(Debug)]
pub(crate) enum EncodeWarning {
    /// The source format cannot be decoded by any available decoder (vector/metafile formats).
    Undecodable {
        /// Format string of the source image (e.g. `"svg"`, `"emf"`).
        source_format: String,
    },
    /// The source bytes failed to decode despite the format being nominally supported.
    DecodeFailed {
        /// Format string that was attempted.
        source_format: String,
        /// Underlying error message from the decoder.
        message: String,
    },
    /// The decoded image could not be encoded to the target format.
    EncodeFailed {
        /// Name of the target format (e.g. `"jpeg"`, `"webp"`).
        target_format: &'static str,
        /// Underlying error message from the encoder.
        message: String,
    },
    /// The encoder for the target format is not available at runtime.
    #[cfg(feature = "heic")]
    EncoderUnavailable {
        /// Name of the target format.
        target_format: &'static str,
        /// Details about why the encoder is unavailable.
        message: String,
    },
    /// The requested conversion direction is not supported (e.g. raster → SVG).
    ///
    /// The image bytes are left untouched.  This is a non-fatal warning: the
    /// pipeline emits the warning and continues with the original image.
    ///
    /// Gated on `svg`: raster→SVG is the only direction the pipeline rejects with
    /// this variant, and that branch only exists when the `svg` feature is active.
    /// Without the gate, Windows/mobile aggregates that omit `svg` trip
    /// `-D dead-code` on the unconstructed variant.
    #[cfg(feature = "svg")]
    UnsupportedDirection {
        /// Format of the source image (e.g. `"jpeg"`, `"png"`).
        from_format: String,
        /// Name of the requested target format (e.g. `"svg"`).
        to_format: &'static str,
    },
}

impl std::fmt::Display for EncodeWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeWarning::Undecodable { source_format } => {
                write!(f, "cannot re-encode format '{source_format}': no decoder available")
            }
            EncodeWarning::DecodeFailed { source_format, message } => {
                write!(f, "failed to decode '{source_format}' image: {message}")
            }
            EncodeWarning::EncodeFailed { target_format, message } => {
                write!(f, "failed to encode image as {target_format}: {message}")
            }
            #[cfg(feature = "heic")]
            EncodeWarning::EncoderUnavailable { target_format, message } => {
                write!(f, "encoder for {target_format} is unavailable: {message}")
            }
            #[cfg(feature = "svg")]
            EncodeWarning::UnsupportedDirection { from_format, to_format } => {
                write!(
                    f,
                    "cannot re-encode '{from_format}' to '{to_format}': direction not supported \
                     (raster sources cannot be vectorized)"
                )
            }
        }
    }
}

/// Re-encode `image` in place to the requested `target` format.
///
/// Returns `Ok(true)` when the image was re-encoded and both `data` and `format`
/// have been updated.  Returns `Ok(false)` when no work was necessary (either the
/// target is [`ImageOutputFormat::Native`] or the image is already in the target
/// format).  On `Err(EncodeWarning)` the image is left completely untouched.
pub(crate) fn re_encode(
    image: &mut ExtractedImage,
    target: ImageOutputFormat,
    limits: &SecurityLimits,
    #[cfg(feature = "svg")] svg_options: &SvgOptions,
) -> Result<bool, EncodeWarning> {
    if target == ImageOutputFormat::Native {
        #[cfg(feature = "svg")]
        if svg_options.sanitize && image.format.eq_ignore_ascii_case("svg") {
            let sanitized = sanitize_svg(&image.data)?;
            image.data = Bytes::from(sanitized.into_bytes());
            return Ok(true);
        }
        return Ok(false);
    }

    if target_matches_format(target, &image.format) {
        return Ok(false);
    }

    #[cfg(feature = "svg")]
    if image.format.eq_ignore_ascii_case("svg") {
        if let ImageOutputFormat::Svg = target {
            let sanitized = sanitize_svg(&image.data)?;
            image.data = Bytes::from(sanitized.into_bytes());
            image.format = Cow::Borrowed("svg");
            return Ok(true);
        }
        let (new_bytes, new_format) = rasterize_svg(&image.data, target, svg_options)?;
        image.data = Bytes::from(new_bytes);
        image.format = Cow::Borrowed(new_format);
        return Ok(true);
    }

    #[cfg(feature = "svg")]
    if let ImageOutputFormat::Svg = target {
        return Err(EncodeWarning::UnsupportedDirection {
            from_format: image.format.to_string(),
            to_format: "svg",
        });
    }

    if is_untranslatable(&image.format) {
        return Err(EncodeWarning::Undecodable {
            source_format: image.format.to_string(),
        });
    }

    let dynamic = decode_source(image, limits)?;
    validate_reencode_peak(&dynamic, target, image.data.len(), limits)?;

    let (new_bytes, new_format) = encode_to_target(&dynamic, target)?;

    image.data = Bytes::from(new_bytes);
    image.format = Cow::Borrowed(new_format);

    Ok(true)
}

const ENCODE_FIXED_OVERHEAD_BYTES: u64 = 256 * 1024;
const PNG_WEBP_ENCODE_BYTES_PER_PIXEL: u64 = 4;
const JPEG_ENCODE_BYTES_PER_PIXEL: u64 = 3;
const HEIF_ENCODE_LIVE_BYTES_PER_PIXEL: u64 = 12;

fn validate_reencode_peak(
    image: &DynamicImage,
    target: ImageOutputFormat,
    encoded_source_bytes: usize,
    limits: &SecurityLimits,
) -> Result<(), EncodeWarning> {
    let additional_bytes_per_pixel = match target {
        ImageOutputFormat::Native => 0,
        ImageOutputFormat::Png | ImageOutputFormat::Webp { .. } => PNG_WEBP_ENCODE_BYTES_PER_PIXEL,
        ImageOutputFormat::Jpeg { .. } => JPEG_ENCODE_BYTES_PER_PIXEL,
        ImageOutputFormat::Heif { .. } => HEIF_ENCODE_LIVE_BYTES_PER_PIXEL,
        #[cfg(feature = "svg")]
        ImageOutputFormat::Svg => 0,
    };
    let encoded_source_bytes = u64::try_from(encoded_source_bytes).unwrap_or(u64::MAX);
    let fixed_live_bytes = ENCODE_FIXED_OVERHEAD_BYTES.saturating_add(encoded_source_bytes);
    validate_dynamic_image_additional_live_bytes(image, limits, additional_bytes_per_pixel, fixed_live_bytes).map_err(
        |error| EncodeWarning::EncodeFailed {
            target_format: "raster",
            message: error.to_string(),
        },
    )
}

/// Returns `true` when `target` already matches the source `format` string,
/// meaning no re-encode is needed.
fn target_matches_format(target: ImageOutputFormat, format: &str) -> bool {
    match target {
        ImageOutputFormat::Native => true,
        ImageOutputFormat::Png => format.eq_ignore_ascii_case("png"),
        ImageOutputFormat::Jpeg { .. } => format.eq_ignore_ascii_case("jpeg") || format.eq_ignore_ascii_case("jpg"),
        ImageOutputFormat::Webp { .. } => format.eq_ignore_ascii_case("webp"),
        #[cfg(feature = "heic")]
        ImageOutputFormat::Heif { .. } => {
            format.eq_ignore_ascii_case("heif")
                || format.eq_ignore_ascii_case("heic")
                || format.eq_ignore_ascii_case("HEIF")
                || format.eq_ignore_ascii_case("HEIC")
        }
        #[cfg(not(feature = "heic"))]
        ImageOutputFormat::Heif { .. } => false,
        #[cfg(feature = "svg")]
        ImageOutputFormat::Svg => false,
    }
}

/// Returns `true` for formats that cannot be decoded by any available decoder.
///
/// These are vector / Windows-metafile formats for which no raster pixel data
/// is accessible.  Returning `Err(EncodeWarning::Undecodable)` signals the
/// pipeline to skip re-encoding and emit a warning instead.
///
/// When the `svg` feature is active, SVG is handled separately (via `sanitize_svg` /
/// `rasterize_svg`) and is therefore **not** listed here.
fn is_untranslatable(format: &str) -> bool {
    let lc = format.to_ascii_lowercase();
    let s = lc.as_str();
    #[cfg(not(feature = "svg"))]
    {
        matches!(s, "svg" | "emf" | "wmf" | "jpeg2000" | "jp2" | "j2k")
    }
    #[cfg(feature = "svg")]
    {
        matches!(s, "emf" | "wmf" | "jpeg2000" | "jp2" | "j2k")
    }
}

/// Parse SVG bytes through `usvg` and re-serialize, stripping external hrefs,
/// JS event handlers, and `foreignObject` elements that `usvg` does not model.
///
/// Minimum allowed `render_dpi` accepted before rasterization.  Values below
/// this are clamped up to avoid degenerate scaling and division-by-zero shapes.
#[cfg(feature = "svg")]
const SVG_RENDER_DPI_MIN: f32 = 1.0;

/// Maximum allowed `render_dpi` accepted before rasterization.  Caps the
/// blast radius of adversarial config combined with a large viewBox.  600 DPI
/// covers print-quality usage; anything beyond is rarely a legitimate need.
#[cfg(feature = "svg")]
const SVG_RENDER_DPI_MAX: f32 = 600.0;

/// Maximum number of output pixels permitted in the rasterized pixmap.
/// `tiny_skia::Pixmap` allocates 4 bytes per pixel (RGBA), so this corresponds
/// to roughly a 1 GB peak allocation — the upper bound at which we'd rather
/// fail loudly than let an adversarial SVG OOM the process.
#[cfg(feature = "svg")]
const SVG_MAX_PIXELS: u64 = 16_384 * 16_384;

/// Maximum input byte length accepted for SVG parse.  usvg expands the source
/// into an in-memory tree synchronously; a small zip-bomb-shape SVG (lots of
/// `<use>` references, gradient stops, or nested `<g>` elements) can blow up
/// CPU and memory before pixmap allocation is even considered.  10 MB is well
/// above any realistic embedded-document SVG.
#[cfg(feature = "svg")]
const SVG_MAX_INPUT_BYTES: usize = 10 * 1024 * 1024;

/// Returns the sanitized SVG as a `String`.  On parse failure returns
/// `Err(EncodeWarning::DecodeFailed)`.
#[cfg(feature = "svg")]
fn sanitize_svg(data: &[u8]) -> Result<String, EncodeWarning> {
    use resvg::usvg;

    if data.len() > SVG_MAX_INPUT_BYTES {
        return Err(EncodeWarning::DecodeFailed {
            source_format: "svg".into(),
            message: format!("SVG input size {} exceeds {SVG_MAX_INPUT_BYTES}-byte cap", data.len()),
        });
    }

    let opts = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_data(data, &opts).map_err(|e| EncodeWarning::DecodeFailed {
        source_format: "svg".to_string(),
        message: e.to_string(),
    })?;

    Ok(tree.to_string(&usvg::WriteOptions::default()))
}

/// Rasterize SVG bytes to a pixel-based format (PNG, JPEG, WebP, HEIF).
///
/// The SVG viewBox is scaled by `svg_options.render_dpi / 96.0` to produce the
/// output pixel dimensions.  The resulting pixel buffer is then handed to the
/// existing raster encode path.
///
/// Returns the encoded bytes and the canonical format name string on success.
#[cfg(feature = "svg")]
fn rasterize_svg(
    data: &[u8],
    target: ImageOutputFormat,
    svg_options: &SvgOptions,
) -> Result<(Vec<u8>, &'static str), EncodeWarning> {
    use resvg::{tiny_skia, usvg};

    if data.len() > SVG_MAX_INPUT_BYTES {
        return Err(EncodeWarning::DecodeFailed {
            source_format: "svg".into(),
            message: format!("SVG input size {} exceeds {SVG_MAX_INPUT_BYTES}-byte cap", data.len()),
        });
    }

    let opts = usvg::Options {
        resources_dir: None,
        dpi: svg_options.render_dpi,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_data(data, &opts).map_err(|e| EncodeWarning::DecodeFailed {
        source_format: "svg".to_string(),
        message: e.to_string(),
    })?;

    let dpi = svg_options.render_dpi.clamp(SVG_RENDER_DPI_MIN, SVG_RENDER_DPI_MAX);
    let scale = dpi / 96.0;
    let svg_size = tree.size();
    let width = ((svg_size.width() * scale) as u32).max(1);
    let height = ((svg_size.height() * scale) as u32).max(1);

    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count > SVG_MAX_PIXELS {
        return Err(EncodeWarning::EncodeFailed {
            target_format: "raster",
            message: format!("SVG render dimensions {width}×{height} exceed {SVG_MAX_PIXELS}-pixel cap"),
        });
    }

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| EncodeWarning::EncodeFailed {
        target_format: "raster",
        message: format!("cannot allocate {width}×{height} pixmap for SVG rasterization"),
    })?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    let rgba_bytes = pixmap.take_demultiplied();
    let rgba_img =
        image::RgbaImage::from_raw(width, height, rgba_bytes).ok_or_else(|| EncodeWarning::EncodeFailed {
            target_format: "raster",
            message: format!("RGBA buffer size mismatch for {width}×{height} SVG rasterization"),
        })?;
    let dynamic = DynamicImage::ImageRgba8(rgba_img);

    encode_to_target(&dynamic, target)
}

/// Decode the source bytes inside `image` to a [`DynamicImage`].
///
/// The dispatch order is:
/// 1. Known format strings → the format-specific `image` decoder
/// 2. `"heic"` / `"heif"` / `"HEIC"` / `"HEIF"` → `xberg-libheif` (feature `heic`)
/// 3. `"unknown"` or anything else → magic-byte auto-detect
fn decode_source(image: &ExtractedImage, limits: &SecurityLimits) -> Result<DynamicImage, EncodeWarning> {
    let format_lc = image.format.to_ascii_lowercase();

    #[cfg(feature = "heic")]
    if matches!(format_lc.as_str(), "heic" | "heif") {
        return decode_heic(&image.data, &format_lc, limits);
    }

    #[cfg(not(feature = "heic"))]
    if matches!(format_lc.as_str(), "heic" | "heif") {
        return Err(EncodeWarning::Undecodable {
            source_format: image.format.to_string(),
        });
    }

    let maybe_fmt: Option<ImageFormat> = match format_lc.as_str() {
        "jpeg" | "jpg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "webp" => Some(ImageFormat::WebP),
        "gif" => Some(ImageFormat::Gif),
        "bmp" => Some(ImageFormat::Bmp),
        "tiff" | "tif" => Some(ImageFormat::Tiff),
        "pnm" | "pbm" | "pgm" | "ppm" => Some(ImageFormat::Pnm),
        _ => None,
    };

    match maybe_fmt {
        Some(format) => {
            decode_standard_image_with_format_and_security_limits(&image.data, format, limits).map_err(|error| {
                EncodeWarning::DecodeFailed {
                    source_format: image.format.to_string(),
                    message: error.to_string(),
                }
            })
        }
        None => decode_standard_image_with_security_limits(&image.data, limits).map_err(|error| match error {
            XbergError::Validation { .. } => EncodeWarning::DecodeFailed {
                source_format: image.format.to_string(),
                message: error.to_string(),
            },
            _ => EncodeWarning::Undecodable {
                source_format: image.format.to_string(),
            },
        }),
    }
}

#[cfg(feature = "heic")]
const HEIF_DECODE_BUFFER_COUNT: u64 = 2;

#[cfg(feature = "heic")]
fn validate_heic_decode_budget(
    width: u32,
    height: u32,
    source_format: &str,
    encoded_source_bytes: usize,
    limits: &SecurityLimits,
) -> Result<(), EncodeWarning> {
    let rgba_bytes = decoded_byte_count(width, height, u64::from(image::ColorType::Rgba8.bytes_per_pixel()))
        .and_then(|bytes| {
            bytes
                .checked_mul(HEIF_DECODE_BUFFER_COUNT)
                .and_then(|peak| peak.checked_add(u64::try_from(encoded_source_bytes).unwrap_or(u64::MAX)))
                .ok_or_else(|| image_dimension_error(width, height, u64::MAX, u64::MAX))
        })
        .and_then(|peak| ImageDecodeBudget::from_security_limits(limits).validate(width, height, peak));
    rgba_bytes.map_err(|error| EncodeWarning::DecodeFailed {
        source_format: source_format.to_string(),
        message: error.to_string(),
    })
}

#[cfg(feature = "heic")]
fn validate_heic_encoded_input_budget(
    encoded_source_bytes: usize,
    source_format: &str,
    limits: &SecurityLimits,
) -> Result<(), EncodeWarning> {
    ImageDecodeBudget::from_security_limits(limits)
        .validate(1, 1, u64::try_from(encoded_source_bytes).unwrap_or(u64::MAX))
        .map_err(|error| EncodeWarning::DecodeFailed {
            source_format: source_format.to_string(),
            message: error.to_string(),
        })
}

/// Decode a HEIC/HEIF image via `xberg-libheif` into a [`DynamicImage`].
///
/// The decoded output is always RGBA8 so that the subsequent encode step has a
/// uniform input regardless of the source chroma.
#[cfg(feature = "heic")]
fn decode_heic(data: &[u8], source_format: &str, limits: &SecurityLimits) -> Result<DynamicImage, EncodeWarning> {
    use xberg_libheif::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    validate_heic_encoded_input_budget(data.len(), source_format, limits)?;
    let context = HeifContext::read_from_bytes(data).map_err(|err| EncodeWarning::DecodeFailed {
        source_format: source_format.to_string(),
        message: format!("{err:?}"),
    })?;

    let handle = context
        .primary_image_handle()
        .map_err(|err| EncodeWarning::DecodeFailed {
            source_format: source_format.to_string(),
            message: format!("{err:?}"),
        })?;

    let width = handle.width();
    let height = handle.height();
    validate_heic_decode_budget(width, height, source_format, data.len(), limits)?;

    let lib = LibHeif::new();
    let heif_img = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|err| EncodeWarning::DecodeFailed {
            source_format: source_format.to_string(),
            message: format!("{err:?}"),
        })?;

    let planes = heif_img.planes();
    let plane = planes.interleaved.as_ref().ok_or_else(|| EncodeWarning::DecodeFailed {
        source_format: source_format.to_string(),
        message: "HEIF image has no interleaved plane".to_string(),
    })?;

    let decoded_width = heif_img.width();
    let decoded_height = heif_img.height();
    if decoded_width != width || decoded_height != height {
        return Err(EncodeWarning::DecodeFailed {
            source_format: source_format.to_string(),
            message: format!(
                "HEIF decoded dimensions {decoded_width}x{decoded_height} do not match declared dimensions {width}x{height}"
            ),
        });
    }

    let rgba_bytes = copy_decoded_rows(
        plane.data,
        plane.stride,
        width,
        height,
        u64::from(image::ColorType::Rgba8.bytes_per_pixel()),
    )
    .map_err(|error| EncodeWarning::DecodeFailed {
        source_format: source_format.to_string(),
        message: error.to_string(),
    })?;

    let rgba_img =
        image::RgbaImage::from_raw(width, height, rgba_bytes).ok_or_else(|| EncodeWarning::DecodeFailed {
            source_format: source_format.to_string(),
            message: format!("RGBA buffer does not fit {width}×{height} image"),
        })?;

    Ok(DynamicImage::ImageRgba8(rgba_img))
}

/// Encode `img` into `target` format and return the raw bytes plus the canonical
/// format name string.
///
/// Returns `Err(EncodeWarning)` if the encode step fails.
fn encode_to_target(img: &DynamicImage, target: ImageOutputFormat) -> Result<(Vec<u8>, &'static str), EncodeWarning> {
    match target {
        ImageOutputFormat::Native => {
            unreachable!("Native target must be handled before encode dispatch")
        }
        ImageOutputFormat::Png => {
            let bytes = encode_png(img)?;
            Ok((bytes, "png"))
        }
        ImageOutputFormat::Jpeg { quality } => {
            let clamped = clamp_quality(quality, "jpeg");
            let bytes = encode_jpeg(img, clamped)?;
            Ok((bytes, "jpeg"))
        }
        ImageOutputFormat::Webp { quality: _ } => {
            let bytes = encode_webp_lossless(img)?;
            Ok((bytes, "webp"))
        }
        #[cfg(feature = "heic")]
        ImageOutputFormat::Heif { quality } => {
            let clamped = clamp_quality(quality, "heif");
            let bytes = encode_heif(img, clamped)?;
            Ok((bytes, "heif"))
        }
        #[cfg(not(feature = "heic"))]
        ImageOutputFormat::Heif { quality: _ } => Err(EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: "heic feature is not enabled in this build".to_string(),
        }),
        #[cfg(feature = "svg")]
        ImageOutputFormat::Svg => {
            unreachable!("raster → SVG must be rejected before reaching encode_to_target")
        }
    }
}

/// Clamp a quality value to `1..=100` and emit a warning when clamping occurs.
fn clamp_quality(quality: u8, format_name: &'static str) -> u8 {
    if quality == 0 {
        warn!(
            target: "xberg::image_encode",
            quality,
            format = format_name,
            "quality 0 is out of range (1–100); clamped to 1"
        );
        return 1;
    }
    if quality > 100 {
        warn!(
            target: "xberg::image_encode",
            quality,
            format = format_name,
            "quality {quality} is out of range (1–100); clamped to 100"
        );
        return 100;
    }
    quality
}

/// Encode `img` as PNG (lossless).
fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, EncodeWarning> {
    let mut buf: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "png",
            message: err.to_string(),
        })?;
    Ok(buf)
}

/// Encode `img` as JPEG at the given quality (1–100).
fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, EncodeWarning> {
    use image::codecs::jpeg::JpegEncoder;
    let mut buf: Vec<u8> = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder.encode_image(img).map_err(|err| EncodeWarning::EncodeFailed {
        target_format: "jpeg",
        message: err.to_string(),
    })?;
    Ok(buf)
}

/// Encode `img` as lossless WebP using the `image` crate's built-in VP8L encoder.
///
/// The `quality` field from [`ImageOutputFormat::Webp`] is intentionally ignored:
/// `image` 0.25 exposes only lossless WebP (VP8L) via `WebPEncoder::new_lossless`.
/// Lossy encoding would require the `webp` crate (libwebp FFI) or a future `image`
/// release that exposes a quality knob on its VP8 encode path.
fn encode_webp_lossless(img: &DynamicImage) -> Result<Vec<u8>, EncodeWarning> {
    let mut buf: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::WebP)
        .map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "webp",
            message: err.to_string(),
        })?;
    Ok(buf)
}

/// Encode `img` as HEIF/HEVC using `xberg-libheif`.
///
/// The pixel data is first converted to RGBA8 (via `DynamicImage::to_rgba8`)
/// and then written into a libheif interleaved plane before encoding.
#[cfg(feature = "heic")]
fn encode_heif(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, EncodeWarning> {
    use xberg_libheif::{
        Channel, ColorSpace, CompressionFormat, EncoderQuality, HeifContext, Image, LibHeif, RgbChroma,
    };

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let mut context = HeifContext::new().map_err(|err| EncodeWarning::EncoderUnavailable {
        target_format: "heif",
        message: format!("HeifContext::new failed: {err:?}"),
    })?;

    let mut heif_img =
        Image::new(width, height, ColorSpace::Rgb(RgbChroma::Rgba)).map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: format!("Image::new failed: {err:?}"),
        })?;

    heif_img
        .create_plane(Channel::Interleaved, width, height, 8)
        .map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: format!("create_plane failed: {err:?}"),
        })?;

    {
        let mut planes = heif_img.planes_mut();
        let plane = planes.interleaved.as_mut().ok_or(EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: "interleaved plane missing after create_plane".to_string(),
        })?;
        let row_size = (width as usize) * 4;
        for (dst_row, src_row) in plane.data.chunks_mut(plane.stride).zip(rgba.chunks_exact(row_size)) {
            dst_row[..row_size].copy_from_slice(src_row);
        }
    }

    let lib = LibHeif::new();
    let mut encoder =
        lib.encoder_for_format(CompressionFormat::Hevc)
            .map_err(|err| EncodeWarning::EncoderUnavailable {
                target_format: "heif",
                message: format!("no HEVC encoder available: {err:?}"),
            })?;

    encoder
        .set_quality(EncoderQuality::Lossy(quality))
        .map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: format!("set_quality failed: {err:?}"),
        })?;

    context
        .encode_image(&heif_img, &mut encoder, None)
        .map_err(|err| EncodeWarning::EncodeFailed {
            target_format: "heif",
            message: format!("encode_image failed: {err:?}"),
        })?;

    context.write_to_bytes().map_err(|err| EncodeWarning::EncodeFailed {
        target_format: "heif",
        message: format!("write_to_bytes failed: {err:?}"),
    })
}

#[cfg(test)]
mod tests;
