//! Per-ecosystem package-size measurement (pip, npm, JVM jars, Ruby gems, WASM bundles,
//! NuGet, Hex, PHP extensions).
//!
//! Split out of `sizes.rs` purely to keep that file under the repo's file-length limit;
//! `use super::*` pulls in the shared kernel (`Result`, `dir_size`, `measure_native_ffi_libs`,
//! etc.) defined there.

use super::*;

/// Measure Python package size via `uv pip show`.
///
/// Packages must be installed in the project .venv via `uv sync --group bench-*`.
/// Returns an error if the package cannot be found or measured.
///
/// For xberg: measures the single package directory (includes native .so).
/// For third-party frameworks (docling, unstructured, mineru, etc.): uses
/// `pip-weigh` to measure the package + full transitive dependency tree in an
/// isolated venv, capturing deps like torch/transformers that dominate the
/// actual installation footprint.
pub(super) fn measure_pip_package(package: &str) -> Result<Option<u64>> {
    if package == "xberg"
        && let Some(size) = measure_pip_package_via_python(package)
    {
        return Ok(Some(size));
    }

    if package != "xberg"
        && let Some(size) = measure_pip_weigh(package)
    {
        return Ok(Some(size));
    }

    if let Some(size) = measure_pip_package_via_python(package) {
        return Ok(Some(size));
    }

    let output = match Command::new("uv").args(["pip", "show", "-f", package]).output() {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_pip_show_size(&stdout, package))
}

/// Use `pip-weigh --json <package>` to measure a package's total installation
/// footprint including all transitive dependencies. pip-weigh creates an
/// isolated venv, installs the package, and measures via .dist-info/RECORD.
/// Returns None if pip-weigh is not installed or the command fails.
fn measure_pip_weigh(package: &str) -> Option<u64> {
    let output = Command::new("pip-weigh").args(["--json", package]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).ok()?;
    json.get("results")?.get(0)?.get("total_size_bytes")?.as_u64()
}

/// Parse pip show -f output to extract package size
fn parse_pip_show_size(stdout: &str, package: &str) -> Option<u64> {
    let location_line = stdout.lines().find(|l| l.starts_with("Location:"))?;
    let location = location_line.strip_prefix("Location:")?.trim();
    let location_path = Path::new(location);

    if let Some(editable_line) = stdout.lines().find(|l| l.starts_with("Editable project location:"))
        && let Some(editable_path) = editable_line
            .strip_prefix("Editable project location:")
            .map(|s| s.trim())
    {
        let project_dir = Path::new(editable_path);
        let pkg_dir = project_dir.join(package.replace('-', "_"));
        if pkg_dir.exists() {
            return Some(dir_size(&pkg_dir));
        }
        if project_dir.exists() {
            return Some(dir_size(project_dir));
        }
    }

    let package_dir = location_path.join(package.replace('-', "_"));
    if package_dir.exists() {
        return Some(dir_size(&package_dir));
    }

    let mut in_files_section = false;
    let mut total_size: u64 = 0;
    let mut found_files = false;
    for line in stdout.lines() {
        if line.starts_with("Files:") {
            in_files_section = true;
            continue;
        }
        if in_files_section {
            let file_rel = line.trim();
            if file_rel.is_empty() {
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }
            let file_path = location_path.join(file_rel);
            if let Ok(metadata) = fs::metadata(&file_path) {
                total_size += metadata.len();
                found_files = true;
            }
        }
    }
    if found_files {
        return Some(total_size);
    }

    None
}

/// Measure npm package size including native addon binary
/// Sums the compiled `.node` addon files directly under `node_crate`, plus its `dist/` output
/// directory (the local dev build layout).
fn measure_xberg_node_addon_size(node_crate: &Path) -> u64 {
    let mut total: u64 = 0;
    if !node_crate.exists() {
        return total;
    }
    if let Ok(entries) = fs::read_dir(node_crate) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.ends_with(".node")
                && let Ok(metadata) = fs::metadata(&path)
            {
                total += metadata.len();
            }
        }
    }
    let dist_dir = node_crate.join("dist");
    if dist_dir.exists() {
        total += dir_size(&dist_dir);
    }
    total
}

/// Sums the platform-specific `.node` addon files under `node_crate/npm/<platform>/` (the
/// published per-platform npm package layout).
fn measure_xberg_npm_platform_packages_size(node_crate: &Path) -> u64 {
    let mut total: u64 = 0;
    let npm_dir = node_crate.join("npm");
    let Ok(entries) = fs::read_dir(&npm_dir) else {
        return total;
    };
    for entry in entries.flatten() {
        let platform_dir = entry.path();
        if !platform_dir.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&platform_dir) else {
            continue;
        };
        for file in files.flatten() {
            if file.path().extension().and_then(|e| e.to_str()) == Some("node")
                && let Ok(metadata) = file.metadata()
            {
                total += metadata.len();
            }
        }
    }
    total
}

/// Falls back to `npm pack --dry-run` for a package this process doesn't build locally.
fn fetch_npm_pack_size(package: &str) -> Option<u64> {
    let output = Command::new("npm")
        .args(["pack", "--dry-run", "--json", package])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = serde_json::from_str::<serde_json::Value>(&stdout).ok()?;
    json.get(0).and_then(|v| v.get("size")).and_then(|v| v.as_u64())
}

pub(super) fn measure_npm_package(package: &str) -> Result<Option<u64>> {
    if package.contains("xberg") && package.contains("node") {
        let node_crate = Path::new("crates/xberg-node");
        let total = measure_xberg_node_addon_size(node_crate) + measure_xberg_npm_platform_packages_size(node_crate);
        if total > 0 {
            return Ok(Some(total));
        }
    }

    Ok(fetch_npm_pack_size(package))
}

/// Measure binary size
pub(super) fn measure_binary(name: &str) -> Result<Option<u64>> {
    let binary_name = match name {
        "xberg-rust" => "xberg",
        s if s.starts_with("xberg-go") => "xberg-go",
        "xberg-c" | "xberg-rust-paddle" => name,
        _ => return Ok(None),
    };

    if matches!(name, "xberg-rust" | "xberg-c" | "xberg-rust-paddle") {
        let target_paths = [
            "target/release/libxberg_ffi.so",
            "target/release/libxberg_ffi.dylib",
            "target/release/xberg_ffi.dll",
            "target/release/libxberg_ffi.a",
            "target/release/xberg",
            "target/debug/xberg",
            "target/release/libxberg.so",
            "target/release/libxberg.dylib",
            "target/release/xberg.dll",
        ];
        for path in target_paths {
            if let Ok(metadata) = fs::metadata(path) {
                return Ok(Some(metadata.len()));
            }
        }
    }

    if name.starts_with("xberg-go") {
        let go_ffi_paths = [
            "target/release/libxberg_ffi.so",
            "target/release/libxberg_ffi.dylib",
            "target/release/xberg_ffi.dll",
        ];
        for path in go_ffi_paths {
            if let Ok(metadata) = fs::metadata(path) {
                return Ok(Some(metadata.len()));
            }
        }
        let ffi_size = measure_native_ffi_libs();
        if ffi_size > 0 {
            return Ok(Some(ffi_size));
        }
        return Ok(None);
    }

    let output = Command::new("which").arg(binary_name).output().ok();

    if let Some(output) = output
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Ok(metadata) = fs::metadata(&path) {
            return Ok(Some(metadata.len()));
        }
    }

    Ok(None)
}

/// Measure JAR size (Apache Tika)
pub(super) fn measure_jar(name: &str) -> Result<Option<u64>> {
    let possible_paths = [
        "/usr/share/java/tika-app.jar",
        "/opt/tika/tika-app.jar",
        "~/.local/share/tika/tika-app.jar",
    ];

    if name.starts_with("tika") {
        for path in possible_paths {
            let expanded = shellexpand::tilde(path);
            let expanded_path: &str = expanded.as_ref();
            if let Ok(metadata) = fs::metadata(expanded_path) {
                return Ok(Some(metadata.len()));
            }
        }

        if let Ok(jar_path) = std::env::var("TIKA_JAR")
            && let Ok(metadata) = fs::metadata(&jar_path)
        {
            return Ok(Some(metadata.len()));
        }

        let libs_dir = Path::new("tools/benchmark-harness/libs");
        if let Ok(entries) = fs::read_dir(libs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.starts_with("tika-app-")
                    && name.ends_with(".jar")
                    && let Ok(metadata) = fs::metadata(&path)
                {
                    return Ok(Some(metadata.len()));
                }
            }
        }
    }

    if name.starts_with("xberg-java") {
        let mut total: u64 = 0;

        let classes_dir = Path::new("packages/java/target/classes");
        if classes_dir.exists() {
            total += dir_size(classes_dir);
        }

        let deps_dir = Path::new("packages/java/target/dependency");
        if deps_dir.exists() {
            total += dir_size(deps_dir);
        }

        let natives_dir = Path::new("packages/java/target/classes/natives");
        if !has_native_extension(natives_dir) {
            total += measure_native_ffi_libs();
        }

        if total > 0 {
            return Ok(Some(total));
        }

        let jar_path = Path::new("packages/java/target/xberg.jar");
        if let Ok(metadata) = fs::metadata(jar_path) {
            return Ok(Some(metadata.len()));
        }
    }

    Ok(None)
}

/// Measure Ruby gem size using bundle show or gem contents
pub(super) fn measure_gem_package(package: &str) -> Result<Option<u64>> {
    let gem_name = match package {
        "xberg" | "xberg-ruby" => "xberg_rb",
        other => other,
    };

    if let Ok(output) = Command::new("bundle").args(["show", gem_name]).output()
        && output.status.success()
    {
        let gem_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !gem_path.is_empty() {
            let path = Path::new(&gem_path);
            if path.exists() {
                return Ok(Some(dir_size(path)));
            }
        }
    }

    if let Ok(output) = Command::new("ruby")
        .arg("-e")
        .arg(format!(
            "puts Gem::Specification.find_by_name('{}').gem_dir rescue nil",
            gem_name
        ))
        .output()
        && output.status.success()
    {
        let gem_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !gem_path.is_empty() {
            let path = Path::new(&gem_path);
            if path.exists() {
                return Ok(Some(dir_size(path)));
            }
        }
    }

    let ruby_pkg = Path::new("packages/ruby/pkg");
    if ruby_pkg.exists() {
        return Ok(Some(dir_size(ruby_pkg)));
    }
    let ruby_lib = Path::new("packages/ruby/lib");
    if ruby_lib.exists() {
        let lib_size = dir_size(ruby_lib);
        let mut total = lib_size;

        let has_substantial_native = has_native_extension(ruby_lib) && lib_size > 5_000_000;
        if !has_substantial_native {
            total += measure_native_ffi_libs();
        }

        if total > 0 {
            return Ok(Some(total));
        }
    }

    Ok(None)
}

/// Measure WebAssembly bundle size
pub(super) fn measure_wasm_bundle(name: &str) -> Result<Option<u64>> {
    let wasm_paths = [
        "packages/wasm/pkg/xberg_bg.wasm",
        "packages/wasm/dist/xberg.wasm",
        "target/wasm32-unknown-unknown/release/xberg.wasm",
        "crates/xberg-wasm/pkg/xberg_wasm_bg.wasm",
    ];

    for path in wasm_paths {
        if let Ok(metadata) = fs::metadata(path) {
            return Ok(Some(metadata.len()));
        }
    }

    if name.contains("wasm") || name.contains("xberg") {
        let node_modules_paths = ["node_modules/@xberg-io/xberg-wasm"];
        for path in node_modules_paths {
            let dir = Path::new(path);
            if dir.exists() {
                return Ok(Some(dir_size(dir)));
            }
        }
    }

    Ok(None)
}

/// Measure .NET NuGet package size
///
/// Checks project build output directories first, then NuGet cache as fallback.
/// Always ensures native FFI libs are included in the total since the .NET
/// package depends on the Rust shared library at runtime.
/// A directory's own footprint, plus the FFI shared library size when the directory doesn't
/// already bundle a native extension (the .NET package depends on it at runtime either way).
fn measure_dir_with_native_fallback(dir: &Path) -> u64 {
    let mut total = dir_size(dir);
    if !has_native_extension(dir) {
        total += measure_native_ffi_libs();
    }
    total
}

fn measure_nuget_from_project_dirs() -> Option<u64> {
    let project_dirs = ["packages/csharp/Xberg", "packages/csharp/Xberg.Native"];
    for proj_dir_str in project_dirs {
        let proj_dir = Path::new(proj_dir_str);
        for config in ["Release", "Debug"] {
            let bin_dir = proj_dir.join("bin").join(config);
            if bin_dir.exists() {
                return Some(measure_dir_with_native_fallback(&bin_dir));
            }
        }
    }
    None
}

fn measure_nuget_from_benchmark_bin() -> Option<u64> {
    for config in ["Release", "Debug"] {
        let bench_bin = Path::new("packages/csharp/Benchmark/bin").join(config);
        if bench_bin.exists() {
            return Some(measure_dir_with_native_fallback(&bench_bin));
        }
    }
    None
}

fn measure_nuget_from_cache() -> Option<u64> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let nuget_cache_paths = [
        format!("{}/.nuget/packages/xberg", home),
        format!("{}/.nuget/packages/xberg.native", home),
    ];
    for path in nuget_cache_paths {
        let dir = Path::new(&path);
        if dir.exists() {
            return Some(measure_dir_with_native_fallback(dir));
        }
    }
    None
}

pub(super) fn measure_nuget_package(name: &str) -> Result<Option<u64>> {
    if !name.starts_with("xberg-csharp") {
        return Ok(None);
    }

    if let Some(total) = measure_nuget_from_project_dirs() {
        return Ok(Some(total));
    }
    if let Some(total) = measure_nuget_from_benchmark_bin() {
        return Ok(Some(total));
    }
    if let Some(total) = measure_nuget_from_cache() {
        return Ok(Some(total));
    }

    let ffi_size = measure_native_ffi_libs();
    if ffi_size > 0 {
        return Ok(Some(ffi_size));
    }

    Ok(None)
}

/// Measure Elixir Hex package size
pub(super) fn measure_hex_package(name: &str) -> Result<Option<u64>> {
    let build_paths = [
        "packages/elixir/_build/prod/lib/xberg",
        "packages/elixir/_build/dev/lib/xberg",
    ];

    for path in build_paths {
        let dir = Path::new(path);
        if dir.exists() {
            return Ok(Some(dir_size(dir)));
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let hex_paths = [
        format!("{}/.hex/packages/hexpm/xberg", home),
        format!("{}/.mix/archives/xberg", home),
    ];

    for path in hex_paths {
        let dir = Path::new(&path);
        if dir.exists() {
            return Ok(Some(dir_size(dir)));
        }
    }

    if name.starts_with("xberg-elixir") {
        let elixir_dir = Path::new("packages/elixir");
        if elixir_dir.exists() {
            return Ok(Some(dir_size(elixir_dir)));
        }
    }

    Ok(None)
}

/// Measure PHP extension size
pub(super) fn measure_php_extension(name: &str) -> Result<Option<u64>> {
    if let Ok(output) = Command::new("php")
        .args(["-r", "echo ini_get('extension_dir');"])
        .output()
        && output.status.success()
    {
        let ext_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let ext_path = Path::new(&ext_dir).join("xberg.so");
        if let Ok(metadata) = fs::metadata(&ext_path) {
            return Ok(Some(metadata.len()));
        }
    }

    let workspace_paths = [
        "packages/php-ext/target/release/libxberg_php.so",
        "packages/php-ext/target/release/libxberg_php.dylib",
        "target/release/libxberg_php.so",
        "target/release/libxberg_php.dylib",
    ];

    for path in workspace_paths {
        if let Ok(metadata) = fs::metadata(path) {
            return Ok(Some(metadata.len()));
        }
    }

    if name.starts_with("xberg-php") {
        let php_dir = Path::new("packages/php-ext");
        if php_dir.exists() {
            return Ok(Some(dir_size(php_dir)));
        }
    }

    Ok(None)
}
