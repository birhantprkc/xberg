//! Native Xberg Rust adapter
//!
//! This adapter uses the Xberg Rust core library directly for maximum performance.
//! It serves as the baseline for comparing language bindings.

use crate::adapter::FrameworkAdapter;
use crate::extract_xberg_file;
use crate::monitoring::{ResourceMonitor, ResourceStats};
use crate::types::{BenchmarkResult, ErrorKind, FrameworkCapabilities, OcrStatus, PerformanceMetrics};
use crate::{Error, Result};
use async_trait::async_trait;
use std::path::Path;
use std::time::{Duration, Instant};
use xberg::{ExtractedDocument, ExtractionConfig, FormatMetadata};

/// Assemble the metrics block every `BenchmarkResult` in this adapter carries.
///
/// Failure rows report zero throughput; the memory and CPU figures are the same monitor
/// statistics in both cases.
fn metrics_from(resource_stats: &ResourceStats, throughput_bytes_per_sec: f64) -> PerformanceMetrics {
    PerformanceMetrics {
        baseline_memory_bytes: resource_stats.baseline_memory_bytes,
        peak_memory_bytes: resource_stats.peak_memory_bytes,
        peak_memory_delta_bytes: resource_stats.peak_memory_delta_bytes,
        avg_cpu_percent: resource_stats.avg_cpu_percent,
        cpu_seconds: resource_stats.cpu_seconds,
        throughput_bytes_per_sec,
        p50_memory_bytes: resource_stats.p50_memory_bytes,
        p95_memory_bytes: resource_stats.p95_memory_bytes,
        p99_memory_bytes: resource_stats.p99_memory_bytes,
    }
}

/// The per-file facts both single-file rows share, independent of extraction success.
struct SingleResultContext<'a> {
    framework: &'a str,
    output_format: crate::types::OutputFormat,
    file_path: &'a Path,
    file_size: u64,
    duration: Duration,
    extraction_duration: Duration,
    resource_stats: &'a ResourceStats,
}

fn single_failure_result(context: SingleResultContext<'_>, error: &Error, timed_out: bool) -> BenchmarkResult {
    let error_kind = if timed_out {
        ErrorKind::Timeout
    } else {
        ErrorKind::HarnessError
    };
    BenchmarkResult {
        framework: context.framework.to_string(),
        output_format: context.output_format,
        file_path: context.file_path.to_path_buf(),
        file_size: context.file_size,
        success: false,
        error_message: Some(error.to_string()),
        error_kind,
        duration: context.duration,
        extraction_duration: Some(context.extraction_duration),
        subprocess_overhead: Some(Duration::ZERO),
        metrics: metrics_from(context.resource_stats, 0.0),
        quality: None,
        iterations: vec![],
        statistics: None,
        cold_start_duration: None,
        file_extension: context
            .file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_lowercase(),
        framework_capabilities: FrameworkCapabilities::default(),
        pdf_metadata: None,
        ocr_status: OcrStatus::Unknown,
        extracted_text: None,
        system_load: None,
    }
}

fn single_success_result(
    context: SingleResultContext<'_>,
    extraction_result: ExtractedDocument,
    ocr_status: OcrStatus,
    throughput: f64,
) -> BenchmarkResult {
    let (success, error_message, error_kind) = if extraction_result.content.trim().is_empty() {
        (
            false,
            Some("Framework returned empty content".to_string()),
            ErrorKind::EmptyContent,
        )
    } else {
        (true, None, ErrorKind::None)
    };

    BenchmarkResult {
        framework: context.framework.to_string(),
        output_format: context.output_format,
        file_path: context.file_path.to_path_buf(),
        file_size: context.file_size,
        success,
        error_message,
        error_kind,
        duration: context.duration,
        extraction_duration: Some(context.extraction_duration),
        subprocess_overhead: Some(Duration::ZERO),
        metrics: metrics_from(context.resource_stats, throughput),
        quality: None,
        iterations: vec![],
        statistics: None,
        cold_start_duration: None,
        file_extension: context
            .file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_lowercase(),
        framework_capabilities: FrameworkCapabilities::default(),
        pdf_metadata: None,
        ocr_status,
        extracted_text: Some(extraction_result.content),
        system_load: None,
    }
}

/// Validate the per-file OCR arrays against `file_paths` and build one `ExtractInput` per file.
///
/// # Errors
///
/// Returns [`Error::Benchmark`] if the OCR arrays do not have one entry per file — a mismatch
/// would silently misalign the overrides.
fn build_batch_inputs(
    file_paths: &[&Path],
    force_ocr: &[bool],
    ocr_languages: &[Option<String>],
    config: &ExtractionConfig,
) -> Result<Vec<xberg::ExtractInput>> {
    if force_ocr.len() != file_paths.len() || ocr_languages.len() != file_paths.len() {
        return Err(Error::Benchmark(format!(
            "native batch config cardinality mismatch for {} files",
            file_paths.len()
        )));
    }

    Ok(file_paths
        .iter()
        .zip(force_ocr)
        .zip(ocr_languages)
        .map(|((path, force_ocr), ocr_language)| build_batch_input(path, *force_ocr, ocr_language.as_deref(), config))
        .collect())
}

/// Turn one successful batch envelope into one row per input file.
///
/// xberg returns successful `results` in discovery order and reports failed inputs separately in
/// `errors`, each tagged with its original request index. Walk inputs in order, emitting a failure
/// row for errored indices and consuming the next success otherwise, so rows stay aligned to
/// `file_paths`. ~keep
fn assemble_batch_rows(
    context: &BatchRowContext<'_>,
    file_paths: &[&Path],
    output: &xberg::ExtractionResult,
    config: &ExtractionConfig,
) -> Vec<BenchmarkResult> {
    let extraction_results = &output.results;
    let error_messages: std::collections::HashMap<usize, String> = output
        .errors
        .iter()
        .map(|item| (item.index, item.message.clone()))
        .collect();
    let mut success_cursor = 0usize;

    file_paths
        .iter()
        .enumerate()
        .map(|(input_index, file_path)| {
            if let Some(message) = error_messages.get(&input_index) {
                return batch_failure_result(context, file_path, message.clone(), ErrorKind::FrameworkError);
            }
            let Some(extraction_result) = extraction_results.get(success_cursor) else {
                return batch_failure_result(
                    context,
                    file_path,
                    "batch output missing a result for this input".to_string(),
                    ErrorKind::FrameworkError,
                );
            };
            success_cursor += 1;
            batch_success_result(context, file_path, extraction_result, config)
        })
        .collect()
}

/// The facts every row of one batch shares.
struct BatchRowContext<'a> {
    framework: &'a str,
    output_format: crate::types::OutputFormat,
    avg_duration_per_file: Duration,
    resource_stats: &'a ResourceStats,
}

/// Build a failed per-file row. Used for both a whole-batch failure and a per-input
/// `output.errors` entry, so both paths carry identical metrics. ~keep
fn batch_failure_result(
    context: &BatchRowContext<'_>,
    file_path: &Path,
    error_message: String,
    error_kind: ErrorKind,
) -> BenchmarkResult {
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    let file_extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_string();
    BenchmarkResult {
        framework: context.framework.to_string(),
        output_format: context.output_format,
        file_path: file_path.to_path_buf(),
        file_size,
        success: false,
        error_message: Some(error_message),
        error_kind,
        duration: context.avg_duration_per_file,
        extraction_duration: Some(context.avg_duration_per_file),
        subprocess_overhead: Some(Duration::ZERO),
        metrics: metrics_from(context.resource_stats, 0.0),
        quality: None,
        iterations: vec![],
        statistics: None,
        cold_start_duration: None,
        file_extension,
        framework_capabilities: FrameworkCapabilities::default(),
        pdf_metadata: None,
        ocr_status: OcrStatus::Unknown,
        extracted_text: None,
        system_load: None,
    }
}

fn batch_success_result(
    context: &BatchRowContext<'_>,
    file_path: &Path,
    extraction_result: &ExtractedDocument,
    config: &ExtractionConfig,
) -> BenchmarkResult {
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    let extraction_duration = extraction_result
        .metadata
        .extraction_duration_ms
        .filter(|&ms| ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(context.avg_duration_per_file);

    let file_throughput = if extraction_duration > Duration::from_secs(0) {
        file_size as f64 / extraction_duration.as_secs_f64()
    } else {
        0.0
    };

    let file_extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();

    let (success, error_message, error_kind) = if extraction_result.metadata.error.is_some() {
        (
            false,
            extraction_result.metadata.error.as_ref().map(|e| e.message.clone()),
            ErrorKind::FrameworkError,
        )
    } else if extraction_result.content.trim().is_empty() {
        (
            false,
            Some("Framework returned empty content".to_string()),
            ErrorKind::EmptyContent,
        )
    } else {
        (true, None, ErrorKind::None)
    };

    BenchmarkResult {
        framework: context.framework.to_string(),
        output_format: context.output_format,
        file_path: file_path.to_path_buf(),
        file_size,
        success,
        error_message,
        error_kind,
        duration: extraction_duration,
        extraction_duration: Some(extraction_duration),
        subprocess_overhead: Some(Duration::ZERO),
        metrics: metrics_from(context.resource_stats, file_throughput),
        quality: None,
        iterations: vec![],
        statistics: None,
        cold_start_duration: None,
        file_extension,
        framework_capabilities: FrameworkCapabilities::default(),
        pdf_metadata: None,
        ocr_status: determine_ocr_status(extraction_result, config),
        extracted_text: Some(extraction_result.content.clone()),
        system_load: None,
    }
}

/// Extensions the native adapter benchmarks. Kept as a flat list rather than a `matches!`
/// arm chain so the set is one readable declaration. ~keep
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "pdf",
    "docx",
    "docm",
    "dotx",
    "dotm",
    "dot",
    "doc",
    "odt",
    "pptx",
    "ppsx",
    "pptm",
    "potx",
    "potm",
    "pot",
    "ppt",
    "xlsx",
    "xlsm",
    "xlsb",
    "xlam",
    "xla",
    "xltx",
    "xlt",
    "xls",
    "ods",
    "dbf",
    "hwp",
    "hwpx",
    "txt",
    "md",
    "markdown",
    "commonmark",
    "html",
    "htm",
    "xml",
    "rtf",
    "rst",
    "org",
    "json",
    "yaml",
    "yml",
    "toml",
    "csv",
    "tsv",
    "eml",
    "msg",
    "zip",
    "tar",
    "gz",
    "tgz",
    "7z",
    "bmp",
    "gif",
    "jpg",
    "jpeg",
    "png",
    "tiff",
    "tif",
    "webp",
    "jp2",
    "jpx",
    "jpm",
    "mj2",
    "j2k",
    "j2c",
    "jbig2",
    "jb2",
    "pnm",
    "pbm",
    "pgm",
    "ppm",
    "epub",
    "fb2",
    "bib",
    "ris",
    "nbib",
    "enw",
    "ipynb",
    "tex",
    "latex",
    "typst",
    "typ",
    "opml",
    "dbk",
    "docbook",
    "jats",
    "svg",
    "djot",
];

/// Determine OCR status by inspecting the actual extraction result metadata.
///
/// The xberg crate sets `FormatMetadata::Ocr` for raw tesseract results, but the
/// image extractor overwrites format to `FormatMetadata::Image` even when OCR was used.
/// So we also check: if the format is `Image` and OCR was enabled in config, OCR was used.
///
/// Returns:
/// - `OcrStatus::Used` if OCR metadata is present, or if this is an image with OCR enabled
/// - `OcrStatus::NotUsed` if format metadata is present and OCR was not involved
/// - `OcrStatus::Unknown` if no format metadata is available
fn determine_ocr_status(result: &ExtractedDocument, config: &ExtractionConfig) -> OcrStatus {
    match &result.metadata.format {
        Some(FormatMetadata::Ocr(_)) => OcrStatus::Used,
        Some(FormatMetadata::Image(_)) => {
            if config.ocr.is_some() || config.force_ocr {
                OcrStatus::Used
            } else {
                OcrStatus::NotUsed
            }
        }
        Some(_) => OcrStatus::NotUsed,
        None => OcrStatus::Unknown,
    }
}

/// Native Rust adapter using xberg crate directly
pub struct NativeAdapter {
    config: ExtractionConfig,
}

impl NativeAdapter {
    /// Create a new native adapter with default configuration
    ///
    /// NOTE: Cache is explicitly disabled for accurate benchmarking
    pub fn new() -> Self {
        let config = ExtractionConfig {
            use_cache: false,
            ..Default::default()
        };
        Self { config }
    }

    /// Clone the adapter's base config and apply the per-fixture OCR overrides.
    fn extraction_config_for(&self, force_ocr: bool, ocr_language: Option<&str>) -> ExtractionConfig {
        let mut config = self.config.clone();
        config.force_ocr |= force_ocr;
        if let Some(language) = ocr_language {
            // Override only the language, preserving the base backend/pipeline/cache. ~keep
            config.ocr.get_or_insert_with(Default::default).language =
                crate::adapter::canonicalize_ocr_languages(language);
        }
        config
    }

    /// Calculate adaptive sampling interval based on estimated task duration from file size
    ///
    /// Uses file size as a proxy for task duration to optimize sampling frequency:
    /// - Small files (<100KB, ~50-100ms tasks): 1ms sampling for high resolution
    /// - Medium files (100KB-1MB, ~100-1000ms tasks): 5ms sampling for balance
    /// - Large files (>1MB, >1000ms tasks): 10ms sampling to reduce overhead
    ///
    /// This adaptive approach ensures:
    /// - Quick tasks: 50-100 samples (sufficient for variance calculation)
    /// - Long tasks: 100-1000+ samples (excellent statistical significance)
    /// - Minimal monitoring overhead for all workloads
    ///
    /// # Arguments
    /// * `file_size` - File size in bytes
    ///
    /// # Returns
    /// Sampling interval in milliseconds (1, 5, or 10)
    fn calculate_adaptive_sampling_interval(file_size: u64) -> u64 {
        crate::monitoring::adaptive_sampling_interval_ms(file_size)
    }

    /// Create a new native adapter with custom configuration
    pub fn with_config(config: ExtractionConfig) -> Self {
        Self { config }
    }
}

impl Default for NativeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FrameworkAdapter for NativeAdapter {
    fn name(&self) -> &str {
        "xberg-rust"
    }

    fn supports_format(&self, file_type: &str) -> bool {
        SUPPORTED_EXTENSIONS.contains(&file_type.to_lowercase().as_str())
    }

    async fn extract(
        &self,
        file_path: &Path,
        timeout: Duration,
        force_ocr: bool,
        ocr_language: Option<&str>,
        output_format: crate::types::OutputFormat,
    ) -> Result<BenchmarkResult> {
        let file_size = std::fs::metadata(file_path).map_err(Error::Io)?.len();

        let config = self.extraction_config_for(force_ocr, ocr_language);

        let monitor = ResourceMonitor::new();
        let sampling_interval_ms = Self::calculate_adaptive_sampling_interval(file_size);
        monitor.start(Duration::from_millis(sampling_interval_ms)).await;

        let start = Instant::now();

        let extraction_start = Instant::now();

        let timed_result = tokio::time::timeout(timeout, extract_xberg_file(file_path, &config)).await;
        let timed_out = timed_result.is_err();
        let extraction_result = match timed_result {
            Ok(inner) => inner.map_err(|e| Error::Benchmark(format!("Extraction failed: {}", e))),
            Err(_) => Err(Error::Timeout(format!("Extraction exceeded {:?}", timeout))),
        };

        let extraction_duration = extraction_start.elapsed();
        let duration = start.elapsed();

        let post_sample = monitor.snapshot_current_memory();
        let mut samples = monitor.stop().await;
        if samples.is_empty() {
            samples.push(post_sample);
        }
        let snapshots = monitor.get_snapshots().await;
        let baseline = monitor.baseline_memory().await;
        let resource_stats = ResourceMonitor::calculate_stats(&samples, &snapshots, baseline);

        let throughput = if duration.as_secs_f64() > 0.0 {
            file_size as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        if let Err(e) = extraction_result {
            return Ok(single_failure_result(
                SingleResultContext {
                    framework: self.name(),
                    output_format,
                    file_path,
                    file_size,
                    duration,
                    extraction_duration,
                    resource_stats: &resource_stats,
                },
                &e,
                timed_out,
            ));
        }

        let extraction_result = extraction_result.unwrap();
        let ocr_status = determine_ocr_status(&extraction_result, &config);

        Ok(single_success_result(
            SingleResultContext {
                framework: self.name(),
                output_format,
                file_path,
                file_size,
                duration,
                extraction_duration,
                resource_stats: &resource_stats,
            },
            extraction_result,
            ocr_status,
            throughput,
        ))
    }

    async fn extract_batch(
        &self,
        file_paths: &[&Path],
        timeout: Duration,
        force_ocr: &[bool],
        ocr_languages: &[Option<String>],
        output_format: crate::types::OutputFormat,
    ) -> Result<Vec<BenchmarkResult>> {
        if file_paths.is_empty() {
            return Ok(Vec::new());
        }
        let config = self.config.clone();
        let inputs = build_batch_inputs(file_paths, force_ocr, ocr_languages, &config)?;

        let total_file_size: u64 = file_paths
            .iter()
            .filter_map(|path| std::fs::metadata(path).ok())
            .map(|m| m.len())
            .sum();

        let monitor = ResourceMonitor::new();
        let sampling_interval_ms = Self::calculate_adaptive_sampling_interval(total_file_size);
        monitor.start(Duration::from_millis(sampling_interval_ms)).await;

        let start = Instant::now();

        let timed_result = tokio::time::timeout(timeout, xberg::extract_batch(inputs, &config)).await;
        let timed_out = timed_result.is_err();
        // Keep the whole envelope: output.errors carries per-input failures (with
        // their original index) that must not be dropped, or successful results
        // would be misattributed to the wrong files when zipped positionally.
        let batch_result = match timed_result {
            Ok(inner) => inner.map_err(|e| Error::Benchmark(format!("Batch extraction failed: {}", e))),
            Err(_) => Err(Error::Timeout(format!("Batch extraction exceeded {:?}", timeout))),
        };

        let total_duration = start.elapsed();

        let samples = monitor.stop().await;
        let snapshots = monitor.get_snapshots().await;
        let baseline = monitor.baseline_memory().await;
        let resource_stats = ResourceMonitor::calculate_stats(&samples, &snapshots, baseline);

        let num_files = file_paths.len() as f64;
        let avg_duration_per_file = Duration::from_secs_f64(total_duration.as_secs_f64() / num_files.max(1.0));

        let row_context = BatchRowContext {
            framework: self.name(),
            output_format,
            avg_duration_per_file,
            resource_stats: &resource_stats,
        };

        if let Err(e) = batch_result {
            let error_kind = if timed_out {
                ErrorKind::Timeout
            } else {
                ErrorKind::HarnessError
            };
            let message = e.to_string();
            let failure_results: Vec<BenchmarkResult> = file_paths
                .iter()
                .map(|file_path| batch_failure_result(&row_context, file_path, message.clone(), error_kind))
                .collect();
            return Ok(failure_results);
        }

        let output = batch_result.unwrap();
        let results = assemble_batch_rows(&row_context, file_paths, &output, &config);

        Ok(results)
    }

    fn supports_batch(&self) -> bool {
        true
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    async fn setup(&self) -> Result<()> {
        let warmup_pdf = tempfile::Builder::new()
            .suffix(".pdf")
            .tempfile()
            .map_err(|e| Error::Benchmark(format!("Failed to create warmup file: {e}")))?;
        std::fs::write(warmup_pdf.path(), minimal_pdf_bytes())
            .map_err(|e| Error::Benchmark(format!("Failed to write warmup file: {e}")))?;
        let _ = extract_xberg_file(warmup_pdf.path(), &self.config).await;
        Ok(())
    }

    async fn teardown(&self) -> Result<()> {
        Ok(())
    }
}

/// Minimal valid PDF document for warmup extractions.
fn minimal_pdf_bytes() -> &'static [u8] {
    b"%PDF-1.0
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/MediaBox[0 0 3 3]/Parent 2 0 R/Resources<<>>>>endobj
xref
0 4
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
trailer<</Size 4/Root 1 0 R>>
startxref
206
%%EOF"
}

/// Build one batch `ExtractInput`, applying per-file OCR overrides on top of the
/// batch `base` config.
///
/// Semantics that keep the batch honest:
/// - `force_ocr` is folded as `base.force_ocr || per_file` so a per-file `false`
///   never disables OCR the base forced on (mirrors the single-file `|=`).
/// - a per-file language clones the base `OcrConfig` and overrides only the
///   (canonicalized) `language`, so backend/pipeline/cache survive — a Paddle/VLM
///   backend is never silently turned into Tesseract.
/// - a file with neither a force flag nor a language gets no per-file config and
///   inherits the base unchanged (`FileExtractionConfig` fields default to "inherit").
fn build_batch_input(
    path: &Path,
    force_ocr: bool,
    ocr_language: Option<&str>,
    base: &ExtractionConfig,
) -> xberg::ExtractInput {
    let mut input = xberg::ExtractInput::from_uri(path.to_string_lossy());
    if force_ocr || ocr_language.is_some() {
        let mut file_config = xberg::FileExtractionConfig {
            force_ocr: Some(base.force_ocr || force_ocr),
            ..Default::default()
        };
        if let Some(language) = ocr_language {
            let mut ocr = base.ocr.clone().unwrap_or_default();
            ocr.language = crate::adapter::canonicalize_ocr_languages(language);
            file_config.ocr = Some(ocr);
        }
        input.config = Some(file_config);
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_native_adapter_creation() {
        let adapter = NativeAdapter::new();
        assert_eq!(adapter.name(), "xberg-rust");
    }

    fn base_config(force_ocr: bool, backend: Option<&str>) -> ExtractionConfig {
        let ocr = backend.map(|backend| xberg::OcrConfig {
            backend: backend.to_string(),
            language: vec!["eng".to_string()],
            ..Default::default()
        });
        ExtractionConfig {
            force_ocr,
            ocr,
            ..Default::default()
        }
    }

    #[test]
    fn build_batch_input_does_not_disable_base_forced_ocr() {
        // Base forces OCR; this file passes force_ocr=false but pins a language.
        // The per-file force_ocr must stay true — never Some(false) over the base.
        let base = base_config(true, Some("tesseract"));
        let input = build_batch_input(Path::new("/a.pdf"), false, Some("deu"), &base);
        let file_config = input.config.expect("file config should be set");
        assert_eq!(file_config.force_ocr, Some(true));
    }

    #[test]
    fn build_batch_input_preserves_backend_and_canonicalizes_language() {
        // A Paddle base must stay Paddle; only the language changes, split on '+'.
        let base = base_config(false, Some("paddle"));
        let input = build_batch_input(Path::new("/a.pdf"), false, Some("deu+eng"), &base);
        let ocr = input.config.expect("file config").ocr.expect("ocr override");
        assert_eq!(ocr.backend, "paddle");
        assert_eq!(ocr.language, vec!["deu".to_string(), "eng".to_string()]);
    }

    #[test]
    fn build_batch_input_without_overrides_inherits_base() {
        // No force flag and no language => no per-file config => inherit the base.
        let base = base_config(false, Some("paddle"));
        let input = build_batch_input(Path::new("/a.pdf"), false, None, &base);
        assert!(input.config.is_none());
    }

    #[test]
    fn build_batch_input_force_ocr_without_language_leaves_ocr_inherited() {
        // Forcing OCR but not pinning a language sets force_ocr but leaves ocr None
        // so the base OcrConfig (backend/pipeline/cache) is inherited untouched.
        let base = base_config(false, Some("paddle"));
        let input = build_batch_input(Path::new("/a.pdf"), true, None, &base);
        let file_config = input.config.expect("file config");
        assert_eq!(file_config.force_ocr, Some(true));
        assert!(file_config.ocr.is_none());
    }

    #[tokio::test]
    async fn test_supports_format() {
        let adapter = NativeAdapter::new();
        assert!(adapter.supports_format("pdf"));
        assert!(adapter.supports_format("docx"));
        assert!(adapter.supports_format("txt"));
        assert!(!adapter.supports_format("unknown"));
    }

    #[tokio::test]
    async fn test_extract_text_file() {
        let adapter = NativeAdapter::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, world!").unwrap();

        let result = adapter
            .extract(&file_path, Duration::from_secs(10), false)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.framework, "xberg-rust");
        assert!(result.duration.as_millis() < 1000);
    }
}
