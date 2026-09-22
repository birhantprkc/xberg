//! `measure-framework-sizes`: measures framework installation sizes.

use benchmark_harness::{Result, measure_framework_sizes, save_framework_sizes};
use std::path::PathBuf;

pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

pub(crate) fn execute(output: PathBuf) -> Result<()> {
    println!("Measuring framework installation sizes...");

    let sizes = measure_framework_sizes()?;

    println!("\nFramework sizes:");
    let mut items: Vec<_> = sizes.iter().collect();
    items.sort_by_key(|(k, _)| *k);

    for (name, info) in &items {
        let size_str = if info.size_bytes > 0 {
            format_size(info.size_bytes)
        } else {
            "unknown".to_string()
        };
        let status = "";
        let sys_str = if info.system_deps_bytes > 0 {
            format!(
                " (pkg: {}, sys: {})",
                format_size(info.package_bytes),
                format_size(info.system_deps_bytes)
            )
        } else {
            String::new()
        };
        println!("  {}: {}{}{} - {}", name, size_str, sys_str, status, info.description);
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(benchmark_harness::Error::Io)?;
    }

    save_framework_sizes(&sizes, &output)?;
    println!("\nSizes written to: {}", output.display());

    Ok(())
}
