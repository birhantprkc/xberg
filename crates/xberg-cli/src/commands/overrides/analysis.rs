//! `--quality`, `--detect-language`, and `--token-reduction` overrides.

#[cfg(feature = "analysis")]
use xberg::{ExtractionConfig, LanguageDetectionConfig};

use super::ExtractionOverrides;

/// Token reduction intensity level.
#[cfg(feature = "analysis")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ReductionLevelArg {
    /// Disable token reduction.
    Off,
    /// Remove only the most obvious filler.
    Light,
    /// Balanced reduction (default when enabled).
    Moderate,
    /// Heavy reduction, may lose some nuance.
    Aggressive,
    /// Maximum compression, lossy.
    Maximum,
}

#[cfg(feature = "analysis")]
impl ReductionLevelArg {
    /// Convert to the string mode expected by `TokenReductionConfig`.
    fn as_mode_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Light => "light",
            Self::Moderate => "moderate",
            Self::Aggressive => "aggressive",
            Self::Maximum => "maximum",
        }
    }
}

impl ExtractionOverrides {
    #[cfg(feature = "analysis")]
    pub(super) fn apply_quality_and_detection(&self, config: &mut ExtractionConfig) {
        if let Some(quality_flag) = self.quality {
            config.enable_quality_processing = quality_flag;
        }
        if let Some(detect_language_flag) = self.detect_language {
            if detect_language_flag {
                config
                    .language_detection
                    .get_or_insert_with(LanguageDetectionConfig::default)
                    .enabled = true;
            } else {
                config.language_detection = None;
            }
        }
    }

    #[cfg(feature = "analysis")]
    pub(super) fn apply_token_reduction(&self, config: &mut ExtractionConfig) {
        if let Some(level) = self.token_reduction {
            config
                .token_reduction
                .get_or_insert_with(xberg::TokenReductionOptions::default)
                .mode = level.as_mode_str().to_string();
        }
    }
}
