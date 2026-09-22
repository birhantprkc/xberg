//! Benchmark runner for executing and collecting results
//!
//! This module orchestrates benchmark execution across multiple fixtures and frameworks,
//! with support for concurrent execution and progress reporting.

use crate::adapter::FrameworkAdapter;
use crate::config::BenchmarkConfig;
use crate::fixture::FixtureManager;
use crate::registry::AdapterRegistry;
use crate::types::{BenchmarkResult, DiskSizeInfo, OutputFormat};
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use helpers::{resolve_cohort_manifest_path, resolve_installation_size};

mod execution;
mod helpers;
mod run;
#[cfg(test)]
mod tests;

type SingleBenchmarkTask = (PathBuf, String, Arc<dyn FrameworkAdapter>, bool, Option<String>);
type BatchBenchmarkEntry = (usize, PathBuf, bool, Option<String>);

/// Orchestrates benchmark execution across fixtures and frameworks
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    registry: AdapterRegistry,
    fixtures: FixtureManager,
    cohort_manifest_path: Option<PathBuf>,
    cold_start_durations: HashMap<String, Duration>,
    framework_sizes: HashMap<String, DiskSizeInfo>,
    output_format: OutputFormat,
    fixed_batch_size: Option<usize>,
}

impl BenchmarkRunner {
    fn select_frameworks(&self, framework_names: &[String]) -> Result<Vec<Arc<dyn FrameworkAdapter>>> {
        if framework_names.is_empty() {
            let mut names = self.registry.adapter_names();
            names.sort();
            return Ok(names
                .into_iter()
                .filter_map(|name| self.registry.get(&name))
                .filter(|adapter| adapter.supported_output_formats().contains(&self.output_format))
                .collect());
        }

        let mut selected = Vec::with_capacity(framework_names.len());
        for name in framework_names {
            let adapter = self
                .registry
                .get(name)
                .ok_or_else(|| Error::Config(format!("requested framework '{name}' is not registered")))?;
            if !adapter.supported_output_formats().contains(&self.output_format) {
                return Err(Error::Config(format!(
                    "framework '{name}' does not support {} output",
                    self.output_format
                )));
            }
            selected.push(adapter);
        }
        Ok(selected)
    }

    async fn setup_frameworks(frameworks: &[Arc<dyn FrameworkAdapter>]) -> Result<()> {
        let mut initialized = Vec::with_capacity(frameworks.len());
        for adapter in frameworks {
            if let Err(error) = adapter.setup().await {
                if let Err(teardown_error) = Self::teardown_frameworks(&initialized).await {
                    eprintln!("Warning: teardown after setup failure also failed: {teardown_error}");
                }
                return Err(error);
            }
            initialized.push(Arc::clone(adapter));
        }
        Ok(())
    }

    async fn teardown_frameworks(frameworks: &[Arc<dyn FrameworkAdapter>]) -> Result<()> {
        let mut first_error = None;
        for adapter in frameworks {
            if let Err(error) = adapter.teardown().await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Create a new benchmark runner
    pub fn new(config: BenchmarkConfig, registry: AdapterRegistry) -> Self {
        Self::with_output_format(config, registry, OutputFormat::Markdown)
    }

    /// Create a new benchmark runner with a specific output format
    pub fn with_output_format(config: BenchmarkConfig, registry: AdapterRegistry, output_format: OutputFormat) -> Self {
        let framework_sizes = match crate::sizes::measure_framework_sizes() {
            Ok(sizes) => {
                if !sizes.is_empty() {
                    eprintln!("Measured disk sizes for {} frameworks", sizes.len());
                }
                sizes
                    .into_iter()
                    .map(|(name, fs)| {
                        (
                            name,
                            DiskSizeInfo {
                                size_bytes: fs.size_bytes,
                                package_bytes: fs.package_bytes,
                                system_deps_bytes: fs.system_deps_bytes,
                                model_bytes: fs.model_bytes,
                                method: fs.method,
                                description: fs.description,
                                system_deps_detail: fs.system_deps_detail,
                            },
                        )
                    })
                    .collect()
            }
            Err(e) => {
                eprintln!("Warning: Failed to measure framework sizes: {}", e);
                HashMap::new()
            }
        };

        Self {
            config,
            registry,
            fixtures: FixtureManager::new(),
            cohort_manifest_path: None,
            cold_start_durations: HashMap::new(),
            framework_sizes,
            output_format,
            fixed_batch_size: None,
        }
    }

    /// Load fixtures from a directory or file
    pub fn load_fixtures(&mut self, path: &PathBuf) -> Result<()> {
        self.cohort_manifest_path = None;
        if path.is_dir() {
            self.fixtures.load_fixtures_from_dir(path)?;
        } else {
            self.fixtures.load_fixture(path)?;
        }
        Ok(())
    }

    /// Load exactly the fixtures declared by an ordered cohort manifest.
    pub fn load_cohort(&mut self, fixture_root: &Path, manifest_path: &Path) -> Result<crate::CohortManifest> {
        self.cohort_manifest_path = None;
        let resolved_manifest_path = resolve_cohort_manifest_path(fixture_root, manifest_path);
        let manifest = crate::CohortManifest::from_file(&resolved_manifest_path).map_err(|error| {
            Error::Config(format!(
                "failed to load cohort manifest from resolved path '{}': {error}",
                resolved_manifest_path.display()
            ))
        })?;
        let fixtures = manifest
            .load_fixtures(fixture_root, &resolved_manifest_path)
            .map_err(|error| {
                Error::Config(format!(
                    "failed to load cohort fixtures using resolved manifest '{}': {error}",
                    resolved_manifest_path.display()
                ))
            })?;
        self.fixtures = fixtures;
        self.cohort_manifest_path = Some(resolved_manifest_path);
        Ok(manifest)
    }

    /// Require native batch invocations to use complete, fixed-size partitions.
    pub fn set_fixed_batch_size(&mut self, batch_size: usize) -> Result<()> {
        if batch_size == 0 {
            return Err(Error::Config("fixed batch size must be greater than zero".to_string()));
        }
        self.fixed_batch_size = Some(batch_size);
        Ok(())
    }

    /// Capture path-safe run provenance for the selected adapters and loaded corpus.
    pub fn capture_provenance(
        &self,
        framework_names: &[String],
        fixture_root: &Path,
        cohort: Option<&crate::CohortManifest>,
        cohort_manifest_path: Option<&Path>,
        fixed_batch_size: Option<usize>,
        models: &[crate::ModelProvenance],
    ) -> Result<crate::RunProvenance> {
        let frameworks = self.select_frameworks(framework_names)?;
        let resolved_cohort_manifest_path = match (cohort, cohort_manifest_path) {
            (None, None) => None,
            (Some(_), Some(_)) => Some(self.cohort_manifest_path.as_deref().ok_or_else(|| {
                Error::Config("cohort provenance requested without a successfully loaded cohort manifest".to_string())
            })?),
            _ => {
                return Err(Error::Config(
                    "cohort provenance requires both a manifest and its supplied path".to_string(),
                ));
            }
        };
        crate::RunProvenance::capture(crate::provenance::ProvenanceInputs {
            config: &self.config,
            output_format: self.output_format,
            fixture_root,
            fixtures: &self.fixtures,
            frameworks: &frameworks,
            cohort,
            cohort_manifest_path: resolved_cohort_manifest_path,
            fixed_batch_size,
            models,
        })
    }

    /// Retain only fixtures for the given shard (1-based index, total shards)
    pub fn apply_shard(&mut self, index: usize, total: usize) {
        self.fixtures.retain_shard(index, total);
    }

    /// Get count of loaded fixtures
    pub fn fixture_count(&self) -> usize {
        self.fixtures.len()
    }

    /// Enrich a benchmark result with framework size information
    ///
    /// # Arguments
    /// * `result` - Mutable reference to benchmark result to enrich
    fn enrich_with_framework_size(&self, result: &mut BenchmarkResult) {
        if let Some(size_info) = resolve_installation_size(&result.framework, &self.framework_sizes) {
            result.framework_capabilities.installation_size = Some(size_info);
        }
    }

    /// Get reference to benchmark configuration
    pub fn config(&self) -> &BenchmarkConfig {
        &self.config
    }
}
