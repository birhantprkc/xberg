use crate::ocr::error::OcrError;
use crate::ocr::shaded_rows::normalize_shaded_rows;
use crate::types::ImagePreprocessingConfig;
use xberg_tesseract::Pix;

const ADAPTIVE_TILE_SIZE: i32 = 32;
const CONTRAST_GAMMA: f32 = 0.5;
const CONTRAST_INPUT_MIN: i32 = 40;
const CONTRAST_INPUT_MAX: i32 = 220;
const DARK_BACKGROUND_MEAN_THRESHOLD: f64 = 100.0;
const DENOISE_KERNEL_SIZE: i32 = 3;
const LIGHT_PIXEL_VALUE_THRESHOLD: u8 = 180;
const MIN_LIGHT_PIXEL_FRACTION_FOR_INVERT: f64 = 0.01;
const POLARITY_SAMPLE_STRIDE: i32 = 4;
const SAUVOLA_FACTOR: f32 = 0.35;
const SAUVOLA_MAX_TILE_DIMENSION: i32 = 512;
const SAUVOLA_WINDOW_HALF_SIZE: i32 = 15;
const UNSHARP_FRACTION: f32 = 0.5;
const UNSHARP_HALF_WIDTH: i32 = 3;

pub(crate) fn should_invert_for_polarity(mean_gray: f64, light_fraction: f64, force_invert: bool) -> bool {
    force_invert
        || (mean_gray < DARK_BACKGROUND_MEAN_THRESHOLD && light_fraction >= MIN_LIGHT_PIXEL_FRACTION_FOR_INVERT)
}

pub(crate) fn preprocess_pix(pix: Pix, config: &ImagePreprocessingConfig) -> Result<Pix, OcrError> {
    crate::core::config_validation::validate_image_preprocessing_config(config).map_err(|error| {
        if let crate::XbergError::Validation { message, .. } = error {
            OcrError::InvalidConfiguration(message)
        } else {
            OcrError::InvalidConfiguration(error.to_string())
        }
    })?;
    let binarization_method = config.binarization_method.to_ascii_lowercase();

    let gray = pix
        .to_grayscale()
        .map_err(preprocessing_error("convert to grayscale"))?;
    // ~keep normalize_shaded_rows only touches `gray`; the `contrast_enhance` path with
    // binarization disabled below reconverts from the original `pix` and does not see it
    // (GH#1785 scope: the fix targets the binarized path, which is the config default).
    let gray = if config.normalize_shaded_rows {
        apply_optional(gray, "normalize shaded rows", normalize_shaded_rows)
    } else {
        gray
    };
    let polarity_stats = gray
        .grayscale_stats(LIGHT_PIXEL_VALUE_THRESHOLD, POLARITY_SAMPLE_STRIDE)
        .ok();
    let should_invert = match polarity_stats {
        Some((mean, light_fraction)) => should_invert_for_polarity(mean, light_fraction, config.invert_colors),
        None => config.invert_colors,
    };

    let binarization_enabled = !matches!(binarization_method.as_str(), "none" | "off");
    let mut processed = if binarization_enabled && should_invert {
        gray.invert().map_err(preprocessing_error("invert colors"))?
    } else if binarization_enabled {
        gray
    } else if config.contrast_enhance {
        drop(gray);
        enhance_non_binarized(pix, should_invert)?
    } else if should_invert {
        gray.invert().map_err(preprocessing_error("invert colors"))?
    } else {
        gray
    };

    if config.denoise {
        processed = apply_optional(processed, "denoise", |source| {
            source.median_filter(DENOISE_KERNEL_SIZE, DENOISE_KERNEL_SIZE)
        });
    }
    if config.contrast_enhance && binarization_enabled {
        processed = apply_optional(processed, "enhance contrast", enhance_contrast);
    }

    if binarization_enabled {
        processed = apply_optional(processed, "binarize", |source| {
            binarize(source, &config.binarization_method)
        });
    }
    if config.deskew && processed.depth() == 1 {
        processed = apply_optional(processed, "deskew", Pix::deskew);
    }
    Ok(processed)
}

fn enhance_non_binarized(pix: Pix, invert: bool) -> Result<Pix, OcrError> {
    let source = if invert {
        let inverted = pix.invert().map_err(preprocessing_error("invert colors"))?;
        drop(pix);
        inverted
    } else {
        pix
    };
    let normalized = source
        .background_normalize()
        .map_err(preprocessing_error("normalize background"))?;
    let sharpened = normalized
        .unsharp_mask(UNSHARP_HALF_WIDTH, UNSHARP_FRACTION)
        .map_err(preprocessing_error("sharpen"))?;
    sharpened
        .to_grayscale()
        .map_err(preprocessing_error("convert to grayscale"))
}

fn enhance_contrast(source: &Pix) -> xberg_tesseract::Result<Pix> {
    let normalized = source.background_normalize()?;
    normalized.contrast_stretch(CONTRAST_GAMMA, CONTRAST_INPUT_MIN, CONTRAST_INPUT_MAX)
}

fn binarize(pix: &Pix, method: &str) -> xberg_tesseract::Result<Pix> {
    match method.to_ascii_lowercase().as_str() {
        "otsu" => pix.otsu_threshold(),
        "adaptive" if pix.width().min(pix.height()) >= ADAPTIVE_TILE_SIZE => {
            pix.adaptive_threshold(ADAPTIVE_TILE_SIZE, ADAPTIVE_TILE_SIZE)
        }
        "adaptive" => {
            tracing::warn!(
                requested_binarization_method = "adaptive",
                width = pix.width(),
                height = pix.height(),
                minimum_dimension = ADAPTIVE_TILE_SIZE,
                fallback_binarization_method = "otsu",
                "image too small for adaptive binarization; falling back to Otsu threshold"
            );
            pix.otsu_threshold()
        }
        "sauvola" if pix.width().min(pix.height()) > SAUVOLA_WINDOW_HALF_SIZE * 2 + 2 => {
            let tile_columns = tile_count(pix.width());
            let tile_rows = tile_count(pix.height());
            pix.sauvola_threshold(SAUVOLA_WINDOW_HALF_SIZE, SAUVOLA_FACTOR, tile_columns, tile_rows)
        }
        "sauvola" => {
            tracing::warn!(
                requested_binarization_method = "sauvola",
                width = pix.width(),
                height = pix.height(),
                minimum_dimension = SAUVOLA_WINDOW_HALF_SIZE * 2 + 2,
                fallback_binarization_method = "otsu",
                "image too small for Sauvola binarization; falling back to Otsu threshold"
            );
            pix.otsu_threshold()
        }
        _ => Err(xberg_tesseract::TesseractError::InvalidParameterError),
    }
}

fn tile_count(dimension: i32) -> i32 {
    dimension
        .saturating_add(SAUVOLA_MAX_TILE_DIMENSION - 1)
        .checked_div(SAUVOLA_MAX_TILE_DIMENSION)
        .unwrap_or(1)
        .max(1)
}

fn apply_optional(
    source: Pix,
    operation: &'static str,
    transform: impl FnOnce(&Pix) -> xberg_tesseract::Result<Pix>,
) -> Pix {
    match transform(&source) {
        Ok(transformed) => transformed,
        Err(error) => {
            tracing::warn!(operation, %error, "OCR image preprocessing step failed; retaining prior raster");
            source
        }
    }
}

fn preprocessing_error(operation: &'static str) -> impl FnOnce(xberg_tesseract::TesseractError) -> OcrError {
    move |error| OcrError::ProcessingFailed(format!("Failed to {operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_preserve_grayscale_when_binarization_is_disabled() {
        let config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "none".to_string(),
            ..Default::default()
        };
        let mut rgb = Vec::with_capacity(64 * 64 * 3);
        for value in 0..(64 * 64) {
            let gray = (value % 256) as u8;
            rgb.extend_from_slice(&[gray, gray, gray]);
        }
        let expected = Pix::from_raw_rgb(&rgb, 64, 64).unwrap().to_grayscale().unwrap();
        let pix = Pix::from_raw_rgb(&rgb, 64, 64).unwrap();

        let processed = preprocess_pix(pix, &config).unwrap();
        let expected_stats = expected.grayscale_stats(192, 1).unwrap();
        let processed_stats = processed.grayscale_stats(192, 1).unwrap();
        let (_, low_threshold_fraction) = processed.grayscale_stats(64, 1).unwrap();
        let (_, high_threshold_fraction) = processed_stats;

        assert_eq!(
            processed.depth(),
            8,
            "disabled binarization must retain grayscale samples"
        );
        assert_eq!(
            processed_stats, expected_stats,
            "none must not enhance grayscale pixels"
        );
        assert!(
            low_threshold_fraction > high_threshold_fraction,
            "none must retain intermediate grayscale levels"
        );
    }

    #[test]
    fn should_accept_off_as_disabled_binarization_alias() {
        let off_config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "off".to_string(),
            ..Default::default()
        };
        let none_config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "none".to_string(),
            ..Default::default()
        };
        let mut rgb = Vec::with_capacity(64 * 64 * 3);
        for value in 0..(64 * 64) {
            let gray = (value % 256) as u8;
            rgb.extend_from_slice(&[gray, gray, gray]);
        }

        let off = preprocess_pix(Pix::from_raw_rgb(&rgb, 64, 64).unwrap(), &off_config).unwrap();
        let none = preprocess_pix(Pix::from_raw_rgb(&rgb, 64, 64).unwrap(), &none_config).unwrap();

        assert_eq!(off.depth(), 8);
        assert_eq!(
            off.grayscale_stats(64, 1).unwrap(),
            none.grayscale_stats(64, 1).unwrap()
        );
        assert_eq!(
            off.grayscale_stats(192, 1).unwrap(),
            none.grayscale_stats(192, 1).unwrap()
        );
    }

    #[test]
    fn should_reject_deskew_without_binarization() {
        let config = ImagePreprocessingConfig {
            deskew: true,
            binarization_method: "none".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(&vec![200; 64 * 64 * 3], 64, 64).unwrap();

        let result = preprocess_pix(pix, &config);

        assert!(matches!(
            result,
            Err(OcrError::InvalidConfiguration(message))
                if message == "deskew must be false when binarization_method is none or off"
        ));
    }

    #[test]
    fn should_reject_unknown_binarization_method() {
        let config = ImagePreprocessingConfig {
            binarization_method: "unknown".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(&vec![200; 64 * 64 * 3], 64, 64).unwrap();

        let result = preprocess_pix(pix, &config);

        assert!(matches!(result, Err(OcrError::InvalidConfiguration(_))));
    }

    /// A `tracing` `Layer` that records every emitted event's level and formatted
    /// `message` field, for GH#1785's silent-adaptive/Sauvola-fallback regression tests.
    #[derive(Clone, Default)]
    struct EventCapture {
        events: std::sync::Arc<std::sync::Mutex<Vec<(tracing::Level, String)>>>,
    }

    impl<S> tracing_subscriber::Layer<S> for EventCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
            struct MessageVisitor(String);
            impl tracing::field::Visit for MessageVisitor {
                fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut visitor = MessageVisitor(String::new());
            event.record(&mut visitor);
            self.events.lock().unwrap().push((*event.metadata().level(), visitor.0));
        }
    }

    fn warn_messages(capture: &EventCapture) -> Vec<String> {
        capture
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| *level == tracing::Level::WARN)
            .map(|(_, message)| message.clone())
            .collect()
    }

    /// GH#1785: an image too small for `adaptive` binarization silently fell back to
    /// Otsu with no observable signal, so a caller who asked for `adaptive` had no way
    /// to tell their request was downgraded. `preprocessing.rs:107-121`'s fallback arm
    /// must now emit a WARN naming both the requested and the fallback method.
    #[test]
    fn should_warn_when_adaptive_binarization_falls_back_to_otsu_for_a_small_image() {
        use tracing_subscriber::layer::SubscriberExt as _;

        let capture = EventCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());

        let small_dimension = ADAPTIVE_TILE_SIZE - 1;
        let config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "adaptive".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(
            &vec![200u8; (small_dimension * small_dimension * 3) as usize],
            small_dimension as u32,
            small_dimension as u32,
        )
        .unwrap();

        let result = tracing::subscriber::with_default(subscriber, || preprocess_pix(pix, &config));

        assert!(result.is_ok(), "the fallback must still produce a usable image");
        let messages = warn_messages(&capture);
        assert!(
            messages
                .iter()
                .any(|message| message.contains("adaptive binarization") && message.contains("Otsu")),
            "expected a WARN naming the adaptive-to-Otsu fallback, got: {messages:?}"
        );
    }

    /// GH#1785: the same silent downgrade for `sauvola` on a small image.
    #[test]
    fn should_warn_when_sauvola_binarization_falls_back_to_otsu_for_a_small_image() {
        use tracing_subscriber::layer::SubscriberExt as _;

        let capture = EventCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());

        let small_dimension = SAUVOLA_WINDOW_HALF_SIZE * 2 + 2;
        let config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "sauvola".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(
            &vec![200u8; (small_dimension * small_dimension * 3) as usize],
            small_dimension as u32,
            small_dimension as u32,
        )
        .unwrap();

        let result = tracing::subscriber::with_default(subscriber, || preprocess_pix(pix, &config));

        assert!(result.is_ok(), "the fallback must still produce a usable image");
        let messages = warn_messages(&capture);
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Sauvola binarization") && message.contains("Otsu")),
            "expected a WARN naming the Sauvola-to-Otsu fallback, got: {messages:?}"
        );
    }

    /// Negative control for the two tests above: an image large enough for adaptive
    /// binarization must NOT emit the fallback warning, proving the assertion is
    /// actually sensitive to image size rather than passing unconditionally.
    #[test]
    fn should_not_warn_when_adaptive_binarization_succeeds_for_a_large_enough_image() {
        use tracing_subscriber::layer::SubscriberExt as _;

        let capture = EventCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());

        let large_dimension = ADAPTIVE_TILE_SIZE * 4;
        let config = ImagePreprocessingConfig {
            deskew: false,
            binarization_method: "adaptive".to_string(),
            ..Default::default()
        };
        let pix = Pix::from_raw_rgb(
            &vec![200u8; (large_dimension * large_dimension * 3) as usize],
            large_dimension as u32,
            large_dimension as u32,
        )
        .unwrap();

        let result = tracing::subscriber::with_default(subscriber, || preprocess_pix(pix, &config));

        assert!(result.is_ok());
        let messages = warn_messages(&capture);
        assert!(
            !messages.iter().any(|message| message.contains("falling back to Otsu")),
            "a large-enough image must not trigger the small-image fallback warning: {messages:?}"
        );
    }
}
