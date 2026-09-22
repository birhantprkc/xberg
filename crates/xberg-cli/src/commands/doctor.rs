//! Doctor command - probe configured backends and report what will actually
//! execute on this host.

use anyhow::{Context, Result};
use std::path::PathBuf;
use xberg::{DoctorCheck, DoctorReport, ProbeStatus, doctor};

use super::config::load_config;
use crate::{WireFormat, style};

/// Fold the outcome of an already-run cache cleanup pass into `report` as a regular
/// check, so the text/JSON/TOON renderers only ever need one output shape. `cleaned` is
/// `None` when cleanup was not requested at all (nothing to report); `Some(None)` when it
/// was requested but not attempted (cache dir override, ownership unverifiable).
fn append_cache_clean_check(report: &mut DoctorReport, cleaned: Option<Option<xberg::doctor::CleanOutcome>>) {
    let Some(cleaned) = cleaned else {
        return;
    };
    report.checks.push(match cleaned {
        Some(outcome) if outcome.failed == 0 => DoctorCheck::pass(
            "cache.clean",
            format!("removed {} stray cache file(s)", outcome.removed),
        ),
        Some(outcome) => DoctorCheck::fail(
            "cache.clean",
            format!(
                "removed {} stray cache file(s), failed to remove {}",
                outcome.removed, outcome.failed
            ),
        ),
        None => DoctorCheck::skip(
            "cache.clean",
            "not attempted: XBERG_CACHE_DIR override in effect and ownership cannot be verified",
        ),
    });
}

/// Render the doctor report as human-readable text.
#[expect(
    clippy::print_stdout,
    reason = "the doctor report is the command's stdout result output"
)]
fn print_text_report(report: &DoctorReport) {
    println!("{}", style::header("Doctor"));
    println!("{}", style::dim("======"));
    for check in &report.checks {
        let marker = match check.status {
            ProbeStatus::Pass => style::success("pass"),
            ProbeStatus::Warn => style::warning("warn"),
            ProbeStatus::Fail => style::error("FAIL"),
            ProbeStatus::Skip => style::dim("skip"),
        };
        println!("{marker} {} — {}", check.name, check.message);
    }

    let count = |status: ProbeStatus| report.checks.iter().filter(|c| c.status == status).count();
    let mut parts = vec![format!("{} passed", count(ProbeStatus::Pass))];
    for (status, noun) in [
        (ProbeStatus::Warn, "warning(s)"),
        (ProbeStatus::Skip, "skipped"),
        (ProbeStatus::Fail, "failed"),
    ] {
        let n = count(status);
        if n > 0 {
            parts.push(format!("{n} {noun}"));
        }
    }
    let n = report.checks.len();
    let noun = if n == 1 { "check" } else { "checks" };
    println!();
    println!("{n} {noun}: {}", parts.join(", "));
}

/// Render the doctor report in the requested wire format.
#[expect(
    clippy::print_stdout,
    reason = "the doctor report is the command's stdout result output"
)]
fn print_report(report: &DoctorReport, format: WireFormat) -> Result<()> {
    match format {
        WireFormat::Text => print_text_report(report),
        WireFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(report).context("Failed to serialize doctor report to JSON")?
            );
        }
        WireFormat::Toon => {
            println!(
                "{}",
                serde_toon::to_string(report).context("Failed to serialize doctor report to TOON")?
            );
        }
    }
    Ok(())
}

/// Execute the doctor command. Exits nonzero when any check fails, so the
/// command is usable as a setup gate in scripts and CI.
pub fn doctor_command(
    config_path: Option<PathBuf>,
    no_config_discovery: bool,
    format: WireFormat,
    clean: bool,
) -> Result<()> {
    let config = load_config(config_path, !no_config_discovery)?;

    // Clean first so the report reflects the post-clean cache state; the
    // cleanup result is a regular check, keeping one output shape per format.
    let cleaned = clean.then(xberg::doctor::clean_obsolete);
    let mut report = doctor(&config);
    append_cache_clean_check(&mut report, cleaned);

    print_report(&report, format)?;

    if !report.is_ok() {
        std::process::exit(1);
    }
    Ok(())
}
