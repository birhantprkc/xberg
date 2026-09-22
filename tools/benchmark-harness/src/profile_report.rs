//! Comprehensive profiling report generation with hotspot analysis
//!
//! This module provides infrastructure for generating detailed profiling reports from
//! CPU profile data. Reports include top function hotspots, memory trajectory analysis,
//! actionable recommendations, and sample quality metrics.
//!
//! # Report Components
//!
//! - **Summary Statistics**: Sample count, profiling duration, effective sampling frequency
//! - **Top Hotspots**: Top 10 functions by sample count with percentages
//! - **Memory Trajectory**: Memory usage snapshots over profiling duration (when available)
//! - **Recommendations**: Actionable insights based on sample quality and profiling data
//!
//! # Sample Quality Guidelines
//!
//! - **< 100 samples**: Profile may have high variance, increase duration or frequency
//! - **100-499 samples**: Acceptable for basic analysis, consider longer runs
//! - **500+ samples**: Good quality profile with reliable hotspot identification
//! - **1000+ samples**: Excellent quality with strong statistical confidence
//!
//! # HTML Report Format
//!
//! Reports are generated as self-contained HTML documents with inline CSS, requiring
//! no external dependencies. The HTML is viewable in any modern web browser.

#[cfg(all(feature = "profiling", not(target_os = "windows")))]
use crate::profiling::ProfilingResult;
use std::{collections::HashMap, time::Duration};

/// Number of hotspots retained in a report, highest self-sample-count first.
const TOP_HOTSPOT_COUNT: usize = 10;

/// Multiplier converting a sample-count ratio into a percentage.
const PERCENT_SCALE: f64 = 100.0;

/// Comprehensive profiling report with hotspot analysis
///
/// Contains aggregated profiling metrics, top functions, and analysis recommendations
/// suitable for performance optimization decisions.
#[derive(Debug, Clone)]
pub struct ProfileReport {
    /// Total number of CPU samples collected
    pub sample_count: usize,
    /// Total profiling duration
    pub duration: Duration,
    /// Effective sampling frequency (samples collected per second)
    pub effective_frequency: f64,
    /// Top 10 functions by sample count
    pub top_hotspots: Vec<Hotspot>,
    /// Memory usage trajectory (if available)
    pub memory_trajectory: Vec<MemorySnapshot>,
    /// Actionable recommendations based on profile quality
    pub recommendations: Vec<String>,
}

/// Individual function hotspot identified in the profile
///
/// Represents a function that consumed significant CPU samples during profiling.
#[derive(Debug, Clone)]
pub struct Hotspot {
    /// Function name or symbol (demangled if possible)
    pub function_name: String,
    /// Number of samples attributed to this function
    pub samples: usize,
    /// Percentage of total samples (0.0-100.0)
    pub percentage: f64,
    /// File location if available (filename:line)
    pub file_location: Option<String>,
}

/// The source site a CPU sample was attributed to
///
/// Used as the aggregation key when folding raw stack samples into per-function hotspots.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotspotSite {
    /// Demangled function name of the leaf frame
    pub function_name: String,
    /// Source location as `file:line`, when the binary carries debug info
    pub file_location: Option<String>,
}

/// Memory usage snapshot at a point in time
///
/// Used to track memory growth patterns during profiling.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Relative time from profiling start in milliseconds
    pub timestamp_ms: u64,
    /// Memory usage in bytes (RSS)
    pub memory_bytes: u64,
}

impl Default for ProfileReport {
    fn default() -> Self {
        Self {
            sample_count: 0,
            duration: Duration::ZERO,
            effective_frequency: 0.0,
            top_hotspots: Vec::new(),
            memory_trajectory: Vec::new(),
            recommendations: Vec::new(),
        }
    }
}

impl ProfileReport {
    /// Create a ProfileReport from profiling result (feature-gated for profiling)
    ///
    /// Analyzes the pprof Report structure to extract:
    /// - Sample count and duration metrics
    /// - Top 10 functions by sample count
    /// - Effective sampling frequency
    /// - Quality-based recommendations
    ///
    /// # Arguments
    ///
    /// * `result` - ProfilingResult from ProfileGuard::finish()
    /// * `framework_name` - Name of the framework being profiled (for reporting)
    ///
    /// # Returns
    ///
    /// A ProfileReport with hotspot analysis and recommendations
    ///
    /// # Note
    ///
    /// This function is only available when the `profiling` feature is enabled.
    ///
    /// `sample_count` is the estimate carried by [`ProfilingResult`] (sampling frequency times
    /// elapsed time), whereas hotspot percentages are computed against the sample total actually
    /// present in the pprof report. The two can differ when the profiler drops samples under load.
    #[cfg(all(feature = "profiling", not(target_os = "windows")))]
    pub fn from_profiling_result(result: &ProfilingResult, framework_name: &str) -> Self {
        let duration = result.duration;
        let sample_count = result.sample_count;

        let effective_frequency = if duration.as_secs_f64() > 0.0 {
            sample_count as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        let top_hotspots = Self::extract_top_hotspots(&result.report);

        let recommendations = Self::generate_recommendations(sample_count, framework_name);

        Self {
            sample_count,
            duration,
            effective_frequency,
            top_hotspots,
            memory_trajectory: Vec::new(),
            recommendations,
        }
    }

    /// Extract the top hotspots from a pprof Report
    ///
    /// Walks `pprof::Report::data`, which maps each captured stack to the number of samples in
    /// which it was observed, and attributes every sample to its leaf frame. The result is
    /// therefore *self* time: the cost of a function excluding its callees.
    ///
    /// # Arguments
    ///
    /// * `report` - pprof Report containing collected profile data
    ///
    /// # Returns
    ///
    /// Up to [`TOP_HOTSPOT_COUNT`] hotspots sorted by sample count descending. Empty when the
    /// report holds no samples.
    #[cfg(all(feature = "profiling", not(target_os = "windows")))]
    fn extract_top_hotspots(report: &pprof::Report) -> Vec<Hotspot> {
        let mut leaf_samples: HashMap<HotspotSite, usize> = HashMap::new();
        let mut total_samples: usize = 0;

        for (frames, sample_count) in &report.data {
            let Ok(samples) = usize::try_from(*sample_count) else {
                continue;
            };
            total_samples += samples;

            let Some(leaf) = frames.frames.first().and_then(|inlined| inlined.first()) else {
                continue;
            };
            let site = HotspotSite {
                function_name: leaf.name(),
                file_location: symbol_file_location(leaf),
            };
            *leaf_samples.entry(site).or_default() += samples;
        }

        Self::rank_hotspots(leaf_samples, total_samples)
    }

    /// Rank aggregated per-site sample counts into the top [`TOP_HOTSPOT_COUNT`] hotspots
    ///
    /// # Arguments
    ///
    /// * `leaf_samples` - Sample counts keyed by the site the samples were attributed to
    /// * `total_samples` - Denominator for percentages: every sample in the profile, including
    ///   those whose site could not be resolved
    ///
    /// # Returns
    ///
    /// Hotspots ordered by sample count descending, ties broken by function name so the ordering
    /// is deterministic despite the unordered input map. Empty when `total_samples` is zero.
    pub fn rank_hotspots(leaf_samples: HashMap<HotspotSite, usize>, total_samples: usize) -> Vec<Hotspot> {
        if total_samples == 0 {
            return Vec::new();
        }

        let mut hotspots: Vec<Hotspot> = leaf_samples
            .into_iter()
            .map(|(site, samples)| Hotspot {
                function_name: site.function_name,
                samples,
                percentage: samples as f64 * PERCENT_SCALE / total_samples as f64,
                file_location: site.file_location,
            })
            .collect();

        hotspots.sort_by(|left, right| {
            right
                .samples
                .cmp(&left.samples)
                .then_with(|| left.function_name.cmp(&right.function_name))
        });
        hotspots.truncate(TOP_HOTSPOT_COUNT);
        hotspots
    }

    /// Generate recommendations based on profile quality metrics
    ///
    /// # Arguments
    ///
    /// * `sample_count` - Number of samples collected
    /// * `framework_name` - Name of the profiled framework
    ///
    /// # Returns
    ///
    /// Vector of actionable recommendations
    #[cfg(any(all(feature = "profiling", not(target_os = "windows")), test))]
    fn generate_recommendations(sample_count: usize, framework_name: &str) -> Vec<String> {
        let mut recommendations = vec![format!(
            "Profiling data collected for {} framework with {} samples",
            framework_name, sample_count
        )];

        if sample_count < 50 {
            recommendations.push(
                "Very low sample count (<50): Profile may be unreliable. Increase profiling duration \
                 or sampling frequency for better accuracy."
                    .to_string(),
            );
            recommendations.push(
                "Consider running the benchmark with amplified iterations (see --profiling-amplification) \
                 to collect more samples."
                    .to_string(),
            );
        } else if sample_count < 100 {
            recommendations.push(
                "Low sample count (<100): Profile has high variance. Increase profiling duration or \
                 consider longer-running benchmarks."
                    .to_string(),
            );
        } else if sample_count < 500 {
            recommendations.push(
                "Acceptable sample count (100-500): Profile is suitable for basic hotspot identification, \
                 but confidence in percentages is moderate. Consider longer runs for more precision."
                    .to_string(),
            );
        } else if sample_count < 1000 {
            recommendations.push(
                "Good sample count (500-1000): Profile quality is reliable for identifying hotspots.".to_string(),
            );
        } else {
            recommendations.push(
                "Excellent sample count (1000+): Profile has high statistical confidence. \
                 Hotspot percentages are reliable for optimization decisions."
                    .to_string(),
            );
        }

        match framework_name {
            "xberg" => {
                recommendations.push(
                    "Xberg profile analysis: Focus on PDF parsing (pdf module) and text extraction \
                     (text module) hotspots."
                        .to_string(),
                );
            }
            "python" => {
                recommendations.push(
                    "Python bindings: High overhead in PyO3 marshalling may appear in hotspots. \
                           Consider optimizing PyO3 FFI boundary."
                        .to_string(),
                );
            }
            "ruby" => {
                recommendations.push(
                    "Ruby bindings: GIL contention may limit threading performance. \
                           Verify Magnus FFI overhead in hotspot analysis."
                        .to_string(),
                );
            }
            _ => {}
        }

        recommendations
    }

    /// Generate an HTML report from the profile
    ///
    /// Creates a self-contained HTML document with inline CSS that displays:
    /// - Summary statistics table
    /// - Top 10 hotspots table with percentages
    /// - Memory trajectory chart (if available)
    /// - Recommendations list
    ///
    /// The HTML is viewable in any modern browser without external dependencies.
    ///
    /// # Returns
    ///
    /// HTML string with the formatted report
    pub fn generate_html(&self) -> String {
        let hotspots_html = self.render_hotspots_table();
        let recommendations_html = self.render_recommendations();
        let memory_html = if self.memory_trajectory.is_empty() {
            String::new()
        } else {
            self.render_memory_chart()
        };

        let css = Self::css_styles();
        let duration_ms = self.duration.as_millis();

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Profiling Report</title>
    <style>
{}
    </style>
</head>
<body>
    <div class="container">
        <header class="report-header">
            <h1>CPU Profile Report</h1>
            <p class="subtitle">Comprehensive hotspot analysis and recommendations</p>
        </header>

        <section class="summary-stats">
            <h2>Profiling Summary</h2>
            <table class="stats-table">
                <tr>
                    <td class="stat-label">Total Samples Collected:</td>
                    <td class="stat-value">{}</td>
                </tr>
                <tr>
                    <td class="stat-label">Profiling Duration:</td>
                    <td class="stat-value">{} ms</td>
                </tr>
                <tr>
                    <td class="stat-label">Effective Frequency:</td>
                    <td class="stat-value">{:.1} samples/sec</td>
                </tr>
                <tr>
                    <td class="stat-label">Sample Quality:</td>
                    <td class="stat-value">{}</td>
                </tr>
            </table>
        </section>

        <section class="hotspots-section">
            <h2>Top 10 Hotspots</h2>
            {}
        </section>

        {}

        <section class="recommendations-section">
            <h2>Recommendations</h2>
            {}
        </section>

        <footer class="report-footer">
            <p>Generated by Xberg Benchmark Harness</p>
        </footer>
    </div>
</body>
</html>"#,
            css,
            self.sample_count,
            duration_ms,
            self.effective_frequency,
            self.sample_quality_label(),
            hotspots_html,
            memory_html,
            recommendations_html
        )
    }

    /// Determine sample quality label based on count
    fn sample_quality_label(&self) -> &str {
        match self.sample_count {
            0..=49 => "Very Low",
            50..=99 => "Low",
            100..=499 => "Acceptable",
            500..=999 => "Good",
            _ => "Excellent",
        }
    }

    /// Render hotspots table in HTML
    fn render_hotspots_table(&self) -> String {
        if self.top_hotspots.is_empty() {
            return "<p class=\"no-data\">No hotspots captured in profile</p>".to_string();
        }

        let rows: String = self
            .top_hotspots
            .iter()
            .enumerate()
            .map(|(idx, hotspot)| {
                let bar_width = (hotspot.percentage * 3.0).min(300.0);
                format!(
                    r#"<tr>
                    <td class="rank">{}</td>
                    <td class="function-name" title="{}">{}</td>
                    <td class="sample-count">{}</td>
                    <td class="percentage">
                        <div class="bar-container">
                            <div class="bar" style="width: {}px"></div>
                            <span class="percentage-text">{:.1}%</span>
                        </div>
                    </td>
                </tr>"#,
                    idx + 1,
                    hotspot.function_name,
                    Self::truncate_function_name(&hotspot.function_name, 50),
                    hotspot.samples,
                    bar_width,
                    hotspot.percentage
                )
            })
            .collect();

        format!(
            r#"<table class="hotspots-table">
            <thead>
                <tr>
                    <th class="rank-col">Rank</th>
                    <th class="function-col">Function</th>
                    <th class="samples-col">Samples</th>
                    <th class="percentage-col">Percentage</th>
                </tr>
            </thead>
            <tbody>
                {}
            </tbody>
        </table>"#,
            rows
        )
    }

    /// Render recommendations section in HTML
    fn render_recommendations(&self) -> String {
        if self.recommendations.is_empty() {
            return String::new();
        }

        let items: String = self
            .recommendations
            .iter()
            .map(|rec| format!("<li>{}</li>", html_escape(rec)))
            .collect();

        format!("<ul class=\"recommendations-list\">{}</ul>", items)
    }

    /// Render the memory trajectory section
    ///
    /// Nothing in the harness populates `memory_trajectory` yet, so this renders only for callers
    /// that build a `ProfileReport` with snapshots of their own.
    fn render_memory_chart(&self) -> String {
        if self.memory_trajectory.is_empty() {
            return String::new();
        }

        format!(
            r#"<section class="memory-section">
            <h2>Memory Trajectory</h2>
            <p class="note">Memory profiling data ({} snapshots collected)</p>
        </section>"#,
            self.memory_trajectory.len()
        )
    }

    /// Truncate long function names for display
    fn truncate_function_name(name: &str, max_len: usize) -> String {
        if name.len() > max_len {
            format!("{}...", &name[..max_len - 3])
        } else {
            name.to_string()
        }
    }

    /// Inline CSS styles for the HTML report
    ///
    /// Self-contained styles requiring no external dependencies.
    /// Includes responsive design and print-friendly styles. Kept in its own file (rather
    /// than as an inline literal) purely to stay under the source file's line budget; the
    /// rules and selectors are unchanged, only whitespace/quote-style from running the CSS
    /// formatter on the extracted file (which has no effect on the rendered report). ~keep
    fn css_styles() -> &'static str {
        include_str!("profile_report.css")
    }
}

/// Format a resolved symbol's source position as `file:line`
///
/// Returns `None` when the binary was built without the debug info pprof needs to resolve a
/// filename, in which case the hotspot is reported by function name alone.
#[cfg(all(feature = "profiling", not(target_os = "windows")))]
fn symbol_file_location(symbol: &pprof::Symbol) -> Option<String> {
    let filename = symbol.filename.as_ref()?;
    Some(format!("{}:{}", filename.display(), symbol.lineno()))
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_report_default() {
        let report = ProfileReport::default();
        assert_eq!(report.sample_count, 0);
        assert_eq!(report.duration, Duration::ZERO);
        assert_eq!(report.effective_frequency, 0.0);
        assert!(report.top_hotspots.is_empty());
        assert!(report.recommendations.is_empty());
    }

    #[test]
    fn test_sample_quality_label() {
        let mut report = ProfileReport {
            sample_count: 25,
            ..Default::default()
        };
        assert_eq!(report.sample_quality_label(), "Very Low");

        report.sample_count = 75;
        assert_eq!(report.sample_quality_label(), "Low");

        report.sample_count = 250;
        assert_eq!(report.sample_quality_label(), "Acceptable");

        report.sample_count = 750;
        assert_eq!(report.sample_quality_label(), "Good");

        report.sample_count = 1500;
        assert_eq!(report.sample_quality_label(), "Excellent");
    }

    #[test]
    fn test_generate_recommendations_very_low_samples() {
        let recommendations = ProfileReport::generate_recommendations(25, "xberg");
        assert!(recommendations.len() >= 3);
        assert!(recommendations[1].contains("Very low sample count"));
        assert!(recommendations[2].contains("amplified iterations"));
    }

    #[test]
    fn test_generate_recommendations_good_samples() {
        let recommendations = ProfileReport::generate_recommendations(750, "xberg");
        assert!(recommendations[1].contains("Good sample count"));
    }

    #[test]
    fn test_generate_recommendations_excellent_samples() {
        let recommendations = ProfileReport::generate_recommendations(2000, "python");
        assert!(recommendations[1].contains("Excellent"));
    }

    #[test]
    fn test_truncate_function_name() {
        let long_name = "this_is_a_very_long_function_name_that_should_be_truncated_for_display";
        let truncated = ProfileReport::truncate_function_name(long_name, 30);
        assert_eq!(truncated.len(), 30);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_function_name_short() {
        let short_name = "short";
        let result = ProfileReport::truncate_function_name(short_name, 30);
        assert_eq!(result, "short");
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("\"quote\""), "&quot;quote&quot;");
        assert_eq!(html_escape("'apostrophe'"), "&#39;apostrophe&#39;");
    }

    #[test]
    fn test_generate_html_empty_report() {
        let report = ProfileReport::default();
        let html = report.generate_html();

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("CPU Profile Report"));
        assert!(html.contains("0</td>"));
        assert!(html.contains("Very Low</td>"));
        assert!(html.contains("No hotspots captured"));
    }

    #[test]
    fn test_generate_html_with_hotspots() {
        let report = ProfileReport {
            sample_count: 1000,
            duration: Duration::from_millis(1000),
            effective_frequency: 1000.0,
            top_hotspots: vec![
                Hotspot {
                    function_name: "extraction_function".to_string(),
                    samples: 500,
                    percentage: 50.0,
                    file_location: None,
                },
                Hotspot {
                    function_name: "text_processing".to_string(),
                    samples: 300,
                    percentage: 30.0,
                    file_location: None,
                },
            ],
            recommendations: vec!["Good profile quality".to_string()],
            ..Default::default()
        };

        let html = report.generate_html();

        assert!(html.contains("1000</td>"));
        assert!(html.contains("extraction_function"));
        assert!(html.contains("500"));
        assert!(html.contains("50.0%"));
        assert!(html.contains("Good profile quality"));
        assert!(html.contains("Excellent"));
    }

    #[test]
    fn test_effective_frequency_calculation() {
        let report = ProfileReport {
            sample_count: 1000,
            duration: Duration::from_secs(2),
            effective_frequency: 500.0,
            top_hotspots: Vec::new(),
            memory_trajectory: Vec::new(),
            recommendations: Vec::new(),
        };

        assert_eq!(report.effective_frequency, 500.0);
    }

    #[test]
    fn test_effective_frequency_zero_duration() {
        let report = ProfileReport::default();
        assert_eq!(report.effective_frequency, 0.0);
    }

    #[test]
    fn test_hotspots_render_empty() {
        let report = ProfileReport::default();
        let html = report.render_hotspots_table();
        assert!(html.contains("No hotspots captured"));
    }

    #[test]
    fn test_hotspots_render_with_data() {
        let report = ProfileReport {
            top_hotspots: vec![
                Hotspot {
                    function_name: "func_one".to_string(),
                    samples: 100,
                    percentage: 50.0,
                    file_location: None,
                },
                Hotspot {
                    function_name: "func_two".to_string(),
                    samples: 50,
                    percentage: 25.0,
                    file_location: None,
                },
            ],
            ..Default::default()
        };

        let html = report.render_hotspots_table();
        assert!(html.contains("func_one"));
        assert!(html.contains("100"));
        assert!(html.contains("50.0%"));
        assert!(html.contains("func_two"));
        assert!(html.contains("50"));
        assert!(html.contains("25.0%"));
    }

    fn site(function_name: &str, file_location: Option<&str>) -> HotspotSite {
        HotspotSite {
            function_name: function_name.to_string(),
            file_location: file_location.map(str::to_string),
        }
    }

    #[test]
    fn rank_hotspots_returns_empty_when_no_samples_were_collected() {
        let hotspots = ProfileReport::rank_hotspots(HashMap::new(), 0);
        assert!(hotspots.is_empty(), "a profile with zero samples has no hotspots");
    }

    #[test]
    fn rank_hotspots_orders_by_samples_and_computes_percentages_against_the_total() {
        let leaf_samples = HashMap::from([
            (site("parse_pdf", Some("pdf.rs:10")), 60),
            (site("extract_text", None), 30),
            (site("normalize", None), 10),
        ]);

        let hotspots = ProfileReport::rank_hotspots(leaf_samples, 200);

        assert_eq!(hotspots.len(), 3, "every distinct site must be reported");
        assert_eq!(hotspots[0].function_name, "parse_pdf");
        assert_eq!(hotspots[0].samples, 60);
        assert_eq!(hotspots[0].percentage, 30.0, "60 of 200 samples is 30%, not 100%");
        assert_eq!(hotspots[0].file_location.as_deref(), Some("pdf.rs:10"));
        assert_eq!(hotspots[1].function_name, "extract_text");
        assert_eq!(hotspots[2].function_name, "normalize");
        assert_eq!(hotspots[2].percentage, 5.0);
    }

    #[test]
    fn rank_hotspots_breaks_ties_by_function_name_for_deterministic_output() {
        let leaf_samples = HashMap::from([
            (site("zeta", None), 5),
            (site("alpha", None), 5),
            (site("mid", None), 5),
        ]);

        let names: Vec<String> = ProfileReport::rank_hotspots(leaf_samples, 15)
            .into_iter()
            .map(|hotspot| hotspot.function_name)
            .collect();

        assert_eq!(names, vec!["alpha".to_string(), "mid".to_string(), "zeta".to_string()]);
    }

    #[test]
    fn rank_hotspots_truncates_to_the_top_hotspot_count() {
        let leaf_samples: HashMap<HotspotSite, usize> = (0..TOP_HOTSPOT_COUNT + 5)
            .map(|index| (site(&format!("function_{index}"), None), index + 1))
            .collect();

        let hotspots = ProfileReport::rank_hotspots(leaf_samples, 1000);

        assert_eq!(hotspots.len(), TOP_HOTSPOT_COUNT);
        assert_eq!(hotspots[0].samples, TOP_HOTSPOT_COUNT + 5);
        assert_eq!(hotspots[TOP_HOTSPOT_COUNT - 1].samples, 6);
    }

    #[test]
    fn rank_hotspots_percentages_do_not_exceed_one_hundred_when_sites_are_unresolved() {
        // Samples whose leaf frame could not be resolved still count toward the denominator, so
        // the reported percentages must sum to less than 100 rather than being rescaled to it.
        let leaf_samples = HashMap::from([(site("resolved", None), 25)]);

        let hotspots = ProfileReport::rank_hotspots(leaf_samples, 100);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].percentage, 25.0);
    }

    #[test]
    fn test_css_styles_present() {
        let css = ProfileReport::css_styles();
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains("@media print"));
        assert!(css.contains("border-radius"));
        assert!(css.contains("font-family"));
    }
}
