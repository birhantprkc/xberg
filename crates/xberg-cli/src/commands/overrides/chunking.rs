//! `--chunk*` and `--chunking-tokenizer` overrides.

#[cfg(any(feature = "core-cli", feature = "analysis"))]
use anyhow::{Result, bail};
#[cfg(any(feature = "core-cli", feature = "analysis"))]
use xberg::ChunkingConfig;
#[cfg(any(feature = "core-cli", feature = "analysis"))]
use xberg::ExtractionConfig;

use super::ExtractionOverrides;

impl ExtractionOverrides {
    /// Reject an invalid `--chunk-size` or `--chunk-overlap` combination.
    #[cfg(any(feature = "core-cli", feature = "analysis"))]
    pub(super) fn validate_chunking(&self) -> Result<()> {
        if let Some(size) = self.chunk_size {
            if size == 0 {
                bail!("Invalid chunk size: {size}. Chunk size must be greater than 0.");
            }
            if size > 1_000_000 {
                bail!(
                    "Invalid chunk size: {size}. Chunk size must be less than 1,000,000 characters to avoid excessive memory usage."
                );
            }
        }

        if let Some(overlap) = self.chunk_overlap
            && let Some(size) = self.chunk_size
            && overlap >= size
        {
            bail!("Invalid chunk overlap: {overlap}. Overlap ({overlap}) must be less than chunk size ({size}).");
        }

        Ok(())
    }

    /// Reject `--chunking-tokenizer` when the binary was not built with the
    /// `chunking-tokenizers` feature.
    #[cfg(all(
        any(feature = "core-cli", feature = "analysis"),
        not(feature = "chunking-tokenizers")
    ))]
    pub(super) fn validate_chunking_tokenizer_feature(&self) -> Result<()> {
        if self.chunking_tokenizer.is_some() {
            bail!(
                "--chunking-tokenizer requires the chunking-tokenizers feature. \
                 Rebuild with --features chunking-tokenizers"
            );
        }
        Ok(())
    }

    /// Apply the `--chunk*` flags onto `config.chunking`.
    ///
    /// Naming any `--chunk-*` field flag (`--chunk-size` or `--chunk-overlap`)
    /// is on its own enough to materialise `config.chunking`
    /// when it is still `None`, matching the `has_*_flag` idiom `apply_ocr` uses for
    /// `--ocr-*` field flags (fixed for OCR in `5921a7cc23`). Before this, a field flag given
    /// without `--chunk true`, `--chunking-tokenizer`, or a config file that already set
    /// `chunking` was silently dropped: `config.chunking` stayed `None`, so the early return
    /// below skipped every field assignment with no warning and no error.
    #[cfg(any(feature = "core-cli", feature = "analysis"))]
    pub(super) fn apply_chunking(&self, config: &mut ExtractionConfig) {
        let chunk = if self.chunking_tokenizer.is_some() && self.chunk.is_none() {
            Some(true)
        } else {
            self.chunk
        };

        if chunk == Some(false) {
            config.chunking = None;
            return;
        }
        if (chunk == Some(true) || self.has_chunk_field_flag()) && config.chunking.is_none() {
            config.chunking = Some(ChunkingConfig::default());
        }

        let Some(chunking) = config.chunking.as_mut() else {
            return;
        };

        if let Some(max_characters) = self.chunk_size {
            chunking.max_characters = max_characters;
        }
        if let Some(overlap) = self.chunk_overlap {
            chunking.overlap = overlap;
        }

        if chunking.overlap >= chunking.max_characters {
            chunking.overlap = chunking.max_characters / 4;
        }

        #[cfg(feature = "chunking-tokenizers")]
        if let Some(ref model) = self.chunking_tokenizer {
            chunking.sizing = xberg::ChunkSizing::Tokenizer {
                model: model.clone(),
                cache_dir: None,
            };
        }
    }

    /// Whether any `--chunk-*` field flag (size or overlap) was given,
    /// independent of `--chunk`/`--chunk true` and `--chunking-tokenizer`.
    #[cfg(any(feature = "core-cli", feature = "analysis"))]
    fn has_chunk_field_flag(&self) -> bool {
        self.chunk_size.is_some() || self.chunk_overlap.is_some()
    }
}
