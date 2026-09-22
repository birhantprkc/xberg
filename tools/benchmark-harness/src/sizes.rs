//! Framework size measurement
//!
//! Measures the installation footprint of document extraction frameworks.
//!
//! Xberg bindings are measured dynamically from local build artifacts.
//! Third-party frameworks use hardcoded verified sizes (package + transitive
//! deps + system deps + auto-downloaded ML models) because dynamic measurement
//! is unreliable: pip-weigh times out for large packages (torch, transformers),
//! and dpkg-query returns partial results when package names vary across distros.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod package_measurements;

/// Information about a framework's disk size
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkSize {
    /// Size in bytes (package + system deps + models combined)
    pub size_bytes: u64,
    /// Package-only size in bytes (Python/npm package + transitive deps)
    #[serde(default)]
    pub package_bytes: u64,
    /// System dependency size in bytes (libreoffice, tesseract, ffmpeg, etc.)
    #[serde(default)]
    pub system_deps_bytes: u64,
    /// ML model size in bytes (auto-downloaded on first use: torch models, OCR weights, etc.)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub model_bytes: u64,
    /// `true` when [`Self::model_bytes`] is `0` because the model cache/store was missing or
    /// empty at measurement time, rather than because a real measurement observed zero bytes of
    /// models. Distinguishes "this framework genuinely ships no models" from "models were not
    /// measured in this environment" (e.g. no ML pipeline has run yet on this machine to populate
    /// the cache) — a reader comparing `model_bytes` across frameworks must be able to tell these
    /// apart, since the second case silently understates the real installed footprint.
    #[serde(default, skip_serializing_if = "is_false")]
    pub model_size_unavailable: bool,
    /// Method used to measure (pip_package, npm_package, binary_size, jar_size, etc.)
    pub method: String,
    /// Human-readable description
    pub description: String,
    /// Breakdown of system dependency sizes by package name.
    /// Populated when runtime measurement via dpkg-query succeeds.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub system_deps_detail: HashMap<String, u64>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Framework size measurement results
pub type FrameworkSizes = HashMap<String, FrameworkSize>;

/// Known frameworks with their measurement methods and descriptions
const FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("xberg-rust", "binary_size", "Native Rust core binary"),
    ("xberg-python", "pip_package", "Python wheel package"),
    ("xberg-node", "npm_package", "Node.js native addon"),
    ("xberg-wasm", "wasm_bundle", "WebAssembly binary"),
    ("xberg-ruby", "gem_package", "Ruby gem native extension"),
    ("xberg-go", "binary_size", "Go binary with CGO"),
    ("xberg-java", "jar_size", "Java JAR with JNI"),
    ("xberg-csharp", "nuget_package", ".NET NuGet package"),
    ("xberg-elixir", "hex_package", "Elixir hex package with NIF"),
    ("xberg-php", "php_extension", "PHP extension"),
    ("xberg-c", "binary_size", "C FFI binding"),
    ("xberg-rust-paddle", "binary_size", "Native Rust core with PaddleOCR"),
    ("docling", "pip_package", "IBM Docling document processing"),
    ("markitdown", "pip_package", "Mark It Down markdown converter"),
    ("unstructured", "pip_package", "Unstructured document processing"),
    ("tika", "jar_size", "Apache Tika content analysis"),
    ("pymupdf4llm", "pip_package", "PyMuPDF for LLM"),
    ("mineru", "pip_package", "MinerU document intelligence"),
    ("liteparse", "binary_size", "LiteParse (run-llama) Rust PDF parser"),
];

/// Verified installation footprints for third-party frameworks.
///
/// Each entry: (name, package_bytes, system_deps_bytes, model_bytes, description).
///
/// - **package_bytes**: Python package + all transitive pip dependencies (from pip-weigh).
/// - **system_deps_bytes**: Required system packages (poppler, libreoffice, JRE, ffmpeg, etc.).
/// - **model_bytes**: ML models auto-downloaded on first use (HuggingFace, PaddleOCR, etc.).
///
/// Values measured on Linux x86_64 (Ubuntu 22.04) in March 2026.
/// Sources: pip-weigh --json, PyPI wheel sizes, HuggingFace model pages, apt show.
const KNOWN_THIRD_PARTY_SIZES: &[(&str, u64, u64, u64, &str)] = &[
    ("pymupdf4llm", 51_500_000, 0, 0, "PyMuPDF for LLM"),
    (
        "markitdown",
        80_000_000,
        125_000_000,
        0,
        "Mark It Down markdown converter",
    ),
    ("tika", 57_000_000, 215_000_000, 0, "Apache Tika content analysis"),
    (
        "docling",
        2_500_000_000,
        0,
        470_000_000,
        "IBM Docling document processing",
    ),
    (
        "unstructured",
        300_000_000,
        840_000_000,
        217_000_000,
        "Unstructured document processing",
    ),
    ("mineru", 2_000_000_000, 0, 650_000_000, "MinerU document intelligence"),
    ("liteparse", 35_000_000, 0, 0, "LiteParse (run-llama) Rust PDF parser"),
];

/// Look up a hardcoded third-party size entry.
fn lookup_known_size(name: &str) -> Option<FrameworkSize> {
    KNOWN_THIRD_PARTY_SIZES
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, pkg, sys, models, desc)| FrameworkSize {
            size_bytes: pkg + sys + models,
            package_bytes: *pkg,
            system_deps_bytes: *sys,
            model_bytes: *models,
            model_size_unavailable: false,
            method: "known_size".to_string(),
            description: desc.to_string(),
            system_deps_detail: HashMap::new(),
        })
}

/// Measure framework sizes.
///
/// Third-party frameworks use hardcoded verified values (package + deps + models).
/// Xberg bindings are measured dynamically from local build artifacts.
/// Frameworks that are not installed are silently skipped.
pub fn measure_framework_sizes() -> Result<FrameworkSizes> {
    let mut sizes = HashMap::new();

    for (name, method, description) in FRAMEWORKS {
        if let Some(known) = lookup_known_size(name) {
            sizes.insert(name.to_string(), known);
            continue;
        }

        // The native xberg CLI is measured with a shipped-vs-model breakdown so
        // benchmark rows can report install size fairly (heuristic rows exclude
        // the on-demand model cache; ML rows include it). ~keep
        if *name == "xberg-rust" {
            match measure_xberg_framework_size(description) {
                Some(fs) => {
                    sizes.insert(name.to_string(), fs);
                }
                None => eprintln!("Size measurement: xberg-rust - binary not found, skipping"),
            }
            continue;
        }

        match measure_framework(name, method) {
            Ok(Some(pkg_size)) => {
                sizes.insert(
                    name.to_string(),
                    FrameworkSize {
                        size_bytes: pkg_size,
                        package_bytes: pkg_size,
                        system_deps_bytes: 0,
                        model_bytes: 0,
                        model_size_unavailable: false,
                        method: method.to_string(),
                        description: description.to_string(),
                        system_deps_detail: HashMap::new(),
                    },
                );
            }
            Ok(None) => {
                eprintln!("Size measurement: {} ({}) - not installed, skipping", name, method);
            }
            Err(e) => {
                eprintln!("Size measurement: {} ({}) - failed: {}", name, method, e);
            }
        }
    }

    Ok(sizes)
}

/// Measure framework sizes, failing if any xberg binding cannot be measured.
///
/// Third-party frameworks always succeed (hardcoded values).
/// Xberg bindings must be measurable or an error is returned.
pub fn measure_framework_sizes_strict() -> Result<FrameworkSizes> {
    let mut sizes = HashMap::new();
    let mut errors = Vec::new();

    for (name, method, description) in FRAMEWORKS {
        if let Some(known) = lookup_known_size(name) {
            sizes.insert(name.to_string(), known);
            continue;
        }

        if *name == "xberg-rust" {
            match measure_xberg_framework_size(description) {
                Some(fs) => {
                    sizes.insert(name.to_string(), fs);
                }
                None => errors.push(format!("{} ({})", name, method)),
            }
            continue;
        }

        match measure_framework(name, method) {
            Ok(Some(pkg_size)) => {
                sizes.insert(
                    name.to_string(),
                    FrameworkSize {
                        size_bytes: pkg_size,
                        package_bytes: pkg_size,
                        system_deps_bytes: 0,
                        model_bytes: 0,
                        model_size_unavailable: false,
                        method: method.to_string(),
                        description: description.to_string(),
                        system_deps_detail: HashMap::new(),
                    },
                );
            }
            Ok(None) | Err(_) => {
                errors.push(format!("{} ({})", name, method));
            }
        }
    }

    if !errors.is_empty() {
        return Err(Error::Benchmark(format!(
            "Failed to measure sizes for frameworks: {}. Install these frameworks or use measure_framework_sizes() for lenient mode.",
            errors.join(", ")
        )));
    }

    Ok(sizes)
}

/// Measure a single framework.
/// Returns Ok(Some(size)) for successful measurement, Ok(None) for frameworks
/// that aren't installed, or Err for measurement failures.
fn measure_framework(name: &str, method: &str) -> Result<Option<u64>> {
    match method {
        "pip_package" => package_measurements::measure_pip_package(extract_package_name(name)),
        "npm_package" => package_measurements::measure_npm_package(extract_package_name(name)),
        // Note: `xberg-rust` is measured separately via `measure_xberg_framework_size`
        // (shipped-vs-model breakdown) before this generic dispatch is reached. ~keep
        "binary_size" => package_measurements::measure_binary(name),
        "jar_size" => package_measurements::measure_jar(name),
        "gem_package" => package_measurements::measure_gem_package(extract_package_name(name)),
        "wasm_bundle" => package_measurements::measure_wasm_bundle(name),
        "nuget_package" => package_measurements::measure_nuget_package(name),
        "hex_package" => package_measurements::measure_hex_package(name),
        "php_extension" => package_measurements::measure_php_extension(name),
        _ => Err(Error::Benchmark(format!("Unknown measurement method: {}", method))),
    }
}

/// Extract Python/npm/gem package name from framework name
fn extract_package_name(framework: &str) -> &str {
    let name = framework.strip_suffix("-batch").unwrap_or(framework);

    match name {
        "xberg-python" => "xberg",
        "xberg-node" => "@xberg-io/xberg",
        "xberg-ruby" => "xberg_rb",
        "docling" => "docling",
        "markitdown" => "markitdown",
        "unstructured" => "unstructured",
        "pymupdf4llm" => "pymupdf4llm",
        "mineru" => "mineru",
        _ => name,
    }
}

/// Measure the native xberg CLI install footprint, split so benchmark rows can
/// report install size fairly against competitors.
///
/// - `package_bytes` = shipped footprint: the compiled `xberg`/`xberg-cli`
///   release binary plus any bundled native libraries (ONNX Runtime, tesseract,
///   tree-sitter). This is what a heuristic-only (no-ML) run needs on disk, and
///   the fair comparison point against model-free tools like LiteParse.
/// - `model_bytes` = the on-demand ML model cache (platform cache dir), pulled on
///   first use by the layout/OCR/embedding paths. Comparable to how Docling's
///   auto-downloaded models are reported separately.
/// - `size_bytes` = `package_bytes + model_bytes` (total, matching the
///   convention used for third-party frameworks).
///
/// Returns `None` if no binary is present.
fn measure_xberg_framework_size(description: &str) -> Option<FrameworkSize> {
    let binary_size = [
        "target/release/xberg",
        "target/release/xberg-cli",
        "target/debug/xberg",
        "target/debug/xberg-cli",
    ]
    .iter()
    .find_map(|path| fs::metadata(path).ok().map(|m| m.len()))
    .filter(|size| *size > 0)?;

    let ffi_size = measure_native_ffi_libs();

    let cache_base = xberg_cache_base();
    let (model_size, model_size_unavailable) = measure_model_cache_size(cache_base.as_deref());
    if model_size_unavailable {
        // xberg's ML pipelines (layout, paddle-ocr, candle-*) always download real model weights
        // on first use, so an observed `0` here can never be a genuine "no models" measurement —
        // it means the cache directory is missing or empty *in this environment* (e.g. no ML
        // pipeline has run yet on this machine to populate it). Reporting it silently as 0 would
        // make xberg's installed footprint look smaller than it really is once ML features are
        // used, so this is surfaced loudly instead of folded into `model_bytes` unremarked.
        eprintln!(
            "Xberg measurement WARNING: model cache at {} is missing or empty -- model_bytes=0 is \
             NOT a verified measurement (ML model weights may simply not have been downloaded on \
             this machine yet); size_bytes for this run excludes model weights",
            cache_base
                .as_deref()
                .map_or_else(|| "<unresolved>".to_string(), |path| path.display().to_string())
        );
    }

    let package_bytes = binary_size + ffi_size;
    eprintln!(
        "Xberg measurement: binary={} bytes, ffi_libs={} bytes, cached_models={} bytes (shipped={}, total={})",
        binary_size,
        ffi_size,
        model_size,
        package_bytes,
        package_bytes + model_size,
    );

    Some(FrameworkSize {
        size_bytes: package_bytes + model_size,
        package_bytes,
        system_deps_bytes: 0,
        model_bytes: model_size,
        model_size_unavailable,
        method: "binary_size".to_string(),
        description: description.to_string(),
        system_deps_detail: HashMap::new(),
    })
}

/// Computes the model cache size and whether that size is a genuine "zero" measurement.
///
/// Returns `(size_bytes, unavailable)`. `unavailable` is `true` whenever `size_bytes` is `0`
/// because the directory could not be resolved, does not exist, or contains no files — as opposed
/// to a directory that was actually walked and found to contain 0 bytes of models, which cannot
/// happen for a populated xberg model cache in practice.
fn measure_model_cache_size(dir: Option<&Path>) -> (u64, bool) {
    let size = dir.filter(|candidate| candidate.exists()).map(dir_size).unwrap_or(0);
    (size, size == 0)
}

/// Resolve the xberg model cache base directory, mirroring the core's
/// `cache_dir::resolve_cache_base`: honor `XBERG_CACHE_DIR`, else the
/// platform-appropriate global cache dir (`dirs::cache_dir()/xberg`), else a
/// CWD-relative `.xberg` fallback.
///
/// This must match the core, or the measured `model_bytes` is wrong — notably
/// on macOS the cache lives at `~/Library/Caches/xberg`, not `~/.cache/xberg`.
/// The final CWD fallback matters too: `crates/xberg/src/cache_dir.rs::resolve_cache_base` falls
/// back to `<cwd>/.xberg` when no cache-dir env var is set and no home directory can be resolved
/// (e.g. a minimal container with `HOME` unset) — this function previously returned `None` in
/// that case, which silently reported `model_bytes: 0` when core would have written the cache to
/// a directory this function never looked at. Adding the same fallback here closes that gap.
fn xberg_cache_base() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("XBERG_CACHE_DIR") {
        return Some(PathBuf::from(env_path));
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(Path::new(&home).join("Library/Caches/xberg"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return Some(Path::new(&local).join("xberg"));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            return Some(Path::new(&xdg).join("xberg"));
        }
        if let Ok(home) = std::env::var("HOME") {
            return Some(Path::new(&home).join(".cache/xberg"));
        }
    }

    // Mirror the core's final fallback so this measurement can't silently diverge from where
    // xberg itself actually writes the model cache when no cache-dir env var or home directory
    // can be resolved (see the doc comment above).
    std::env::current_dir().ok().map(|cwd| cwd.join(".xberg"))
}

/// Measure the native FFI library from target/release/.
/// Returns the total size of found native libs, or 0 if none are found.
/// Only counts one platform variant of each library (first match wins).
fn measure_native_ffi_libs() -> u64 {
    let mut total = 0u64;

    for path in [
        "target/release/libxberg_ffi.so",
        "target/release/libxberg_ffi.dylib",
        "target/release/xberg_ffi.dll",
    ] {
        if let Ok(m) = fs::metadata(path) {
            total += m.len();
            break;
        }
    }

    total
}

/// Measure a pip package by asking Python where it is installed.
/// This handles editable installs (maturin develop) where the native .so
/// is in the site-packages directory alongside the Python source files.
fn measure_pip_package_via_python(package: &str) -> Option<u64> {
    let module_name = package.replace('-', "_");
    let script = format!(
        "import {mod_name}, os; print(os.path.dirname({mod_name}.__file__))",
        mod_name = module_name
    );
    let output = Command::new("python3").args(["-c", &script]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let pkg_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pkg_dir.is_empty() {
        return None;
    }

    let path = Path::new(&pkg_dir);
    if path.exists() {
        let size = dir_size(path);
        if size > 10_000 {
            return Some(size);
        }
    }

    None
}

/// Check if a directory (or one level of subdirectories) contains native
/// extension files (.so, .bundle, .dylib, .dll, .node).
fn has_native_extension(dir: &Path) -> bool {
    has_native_extension_inner(dir, 0)
}

fn has_native_extension_inner(dir: &Path, depth: u32) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && matches!(ext, "so" | "bundle" | "dylib" | "dll" | "node")
        {
            return true;
        } else if path.is_dir() && depth < 2 && has_native_extension_inner(&path, depth + 1) {
            return true;
        }
    }
    false
}

/// Calculate total size of a directory
fn dir_size(path: &Path) -> u64 {
    let mut size = 0;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                size += dir_size(&path);
            } else if let Ok(metadata) = path.metadata() {
                size += metadata.len();
            }
        }
    }

    size
}

/// Load framework sizes from a JSON file
pub fn load_framework_sizes(path: &Path) -> Result<FrameworkSizes> {
    let contents = fs::read_to_string(path).map_err(Error::Io)?;
    serde_json::from_str(&contents).map_err(|e| Error::Benchmark(format!("Invalid JSON: {}", e)))
}

/// Save framework sizes to a JSON file
pub fn save_framework_sizes(sizes: &FrameworkSizes, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(sizes)
        .map_err(|e| Error::Benchmark(format!("JSON serialization failed: {}", e)))?;
    fs::write(path, json).map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_package_name() {
        assert_eq!(extract_package_name("xberg-python"), "xberg");
        assert_eq!(extract_package_name("docling"), "docling");
        assert_eq!(extract_package_name("docling-batch"), "docling");
        assert_eq!(extract_package_name("mineru-batch"), "mineru");
    }

    #[test]
    fn test_frameworks_list_complete() {
        assert_eq!(FRAMEWORKS.len(), 19);

        let names: Vec<&str> = FRAMEWORKS.iter().map(|(n, _, _)| *n).collect();
        assert!(names.contains(&"xberg-rust"));
        assert!(names.contains(&"xberg-python"));
        assert!(names.contains(&"xberg-node"));

        assert!(names.contains(&"docling"));
        assert!(names.contains(&"tika"));
        assert!(names.contains(&"unstructured"));
    }

    #[test]
    fn test_dir_size_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let size = dir_size(temp.path());
        assert_eq!(size, 0);
    }

    #[test]
    fn test_dir_size_with_files() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "hello").unwrap();
        fs::write(temp.path().join("b.txt"), "world!").unwrap();

        let size = dir_size(temp.path());
        assert_eq!(size, 11);
    }

    #[test]
    fn model_cache_size_reports_unavailable_when_directory_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let missing = temp.path().join("does-not-exist");

        let (size, unavailable) = measure_model_cache_size(Some(&missing));

        assert_eq!(size, 0);
        assert!(
            unavailable,
            "a missing cache directory must be reported unavailable, not a measured zero"
        );
    }

    #[test]
    fn model_cache_size_reports_unavailable_when_directory_empty() {
        let temp = tempfile::TempDir::new().unwrap();

        let (size, unavailable) = measure_model_cache_size(Some(temp.path()));

        assert_eq!(size, 0);
        assert!(
            unavailable,
            "an empty cache directory must be reported unavailable, not a measured zero"
        );
    }

    #[test]
    fn model_cache_size_reports_unavailable_when_no_directory_resolved() {
        let (size, unavailable) = measure_model_cache_size(None);

        assert_eq!(size, 0);
        assert!(
            unavailable,
            "an unresolved cache directory must be reported unavailable"
        );
    }

    #[test]
    fn model_cache_size_reports_available_when_directory_has_files() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("layout_model.onnx"), vec![0u8; 4096]).unwrap();

        let (size, unavailable) = measure_model_cache_size(Some(temp.path()));

        assert_eq!(size, 4096);
        assert!(!unavailable, "a populated cache directory must be a real measurement");
    }

    #[test]
    fn test_measure_native_ffi_libs_does_not_panic() {
        let _size = measure_native_ffi_libs();
    }

    #[test]
    fn test_measure_pip_package_via_python_nonexistent() {
        let result = measure_pip_package_via_python("nonexistent_package_xyz_123");
        assert!(result.is_none());
    }

    #[test]
    fn test_has_native_extension_empty_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        assert!(!has_native_extension(temp.path()));
    }

    #[test]
    fn test_has_native_extension_with_so() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("module.so"), "fake").unwrap();
        assert!(has_native_extension(temp.path()));
    }

    #[test]
    fn test_has_native_extension_nested() {
        let temp = tempfile::TempDir::new().unwrap();
        let sub = temp.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("lib.dylib"), "fake").unwrap();
        assert!(has_native_extension(temp.path()));
    }

    #[test]
    fn test_has_native_extension_no_match() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "text").unwrap();
        fs::write(temp.path().join("lib.py"), "python").unwrap();
        assert!(!has_native_extension(temp.path()));
    }

    #[test]
    fn test_known_third_party_sizes_all_present() {
        let known_names: Vec<&str> = KNOWN_THIRD_PARTY_SIZES.iter().map(|(n, ..)| *n).collect();
        for (name, _, _) in FRAMEWORKS {
            if !name.starts_with("xberg-") {
                assert!(
                    known_names.contains(name),
                    "Third-party framework '{}' missing from KNOWN_THIRD_PARTY_SIZES",
                    name,
                );
            }
        }
    }

    #[test]
    fn test_known_sizes_are_reasonable() {
        for (name, pkg, sys, models, _) in KNOWN_THIRD_PARTY_SIZES {
            let total = pkg + sys + models;
            assert!(total > 0, "Framework '{}' has zero total size", name,);
            assert!(
                total < 10_000_000_000,
                "Framework '{}' total {} bytes seems too large",
                name,
                total,
            );
        }
    }

    #[test]
    fn test_lookup_known_size_found() {
        let size = lookup_known_size("pymupdf4llm").unwrap();
        assert_eq!(size.package_bytes, 51_500_000);
        assert_eq!(size.system_deps_bytes, 0);
        assert_eq!(size.model_bytes, 0);
        assert_eq!(size.size_bytes, 51_500_000);
        assert_eq!(size.method, "known_size");
    }

    #[test]
    fn test_lookup_known_size_not_found() {
        assert!(lookup_known_size("xberg-rust").is_none());
        assert!(lookup_known_size("nonexistent").is_none());
    }

    #[test]
    fn test_docling_includes_models() {
        let size = lookup_known_size("docling").unwrap();
        assert!(size.model_bytes > 0, "docling should have model_bytes > 0");
        assert_eq!(
            size.size_bytes,
            size.package_bytes + size.system_deps_bytes + size.model_bytes
        );
    }
}
