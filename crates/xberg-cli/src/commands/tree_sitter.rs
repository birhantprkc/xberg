//! Tree-sitter grammar management commands.
//!
//! This module provides commands for downloading, listing, and managing
//! tree-sitter grammar parsers via the tree-sitter-language-pack crate.

use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;

use crate::commands::config::load_config;
use crate::{WireFormat, style};

/// Merge CLI-supplied tree-sitter grammar options with a [`xberg::TreeSitterConfig`]
/// loaded from a config file, honouring the documented config cascade: CLI
/// args > environment > config file > defaults (there is no tree-sitter
/// environment layer, so this only resolves CLI vs. config file).
///
/// Explicit CLI values always win. An empty CLI collection (`cli_languages`
/// empty, or `cli_groups` absent/empty) falls through to the config file's
/// value instead of clobbering it with an empty override.
fn resolve_pack_config(
    cli_cache_dir: Option<PathBuf>,
    cli_languages: &[String],
    cli_groups: Option<&[String]>,
    file_config: Option<&xberg::TreeSitterConfig>,
) -> tree_sitter_language_pack::PackConfig {
    let cache_dir = cli_cache_dir.or_else(|| file_config.and_then(|c| c.cache_dir.clone()));

    let languages = if cli_languages.is_empty() {
        file_config.and_then(|c| c.languages.clone())
    } else {
        Some(cli_languages.to_vec())
    };

    let groups = match cli_groups {
        Some(cli_groups) if !cli_groups.is_empty() => Some(cli_groups.to_vec()),
        _ => file_config.and_then(|c| c.groups.clone()),
    };

    tree_sitter_language_pack::PackConfig {
        cache_dir,
        languages,
        groups,
    }
}

/// Outcome of a tree-sitter grammar download attempt: how many grammars were newly
/// downloaded (0 for a group download, which doesn't report a count) and a human-readable
/// description of what was requested.
struct DownloadOutcome {
    count: usize,
    description: String,
}

/// Point the tree-sitter-language-pack crate at a custom cache directory, if one was
/// resolved from CLI args or the config file.
fn configure_pack_cache_dir(effective_cache_dir: Option<&PathBuf>) -> Result<()> {
    let Some(dir) = effective_cache_dir else {
        return Ok(());
    };
    let config = tree_sitter_language_pack::PackConfig {
        cache_dir: Some(dir.clone()),
        languages: None,
        groups: None,
    };
    tree_sitter_language_pack::configure(&config).context("Failed to configure custom cache directory")
}

/// Download the grammars selected by `pack_config`/`all`, in the same precedence order as
/// the CLI flags: `--all` first, then `--groups`, then explicit languages.
fn download_selected_grammars(
    pack_config: &tree_sitter_language_pack::PackConfig,
    all: bool,
    effective_cache_dir: Option<&PathBuf>,
) -> Result<DownloadOutcome> {
    if all {
        let count = tree_sitter_language_pack::download_all().context("Failed to download all tree-sitter grammars")?;
        return Ok(DownloadOutcome {
            count,
            description: "all available languages".to_string(),
        });
    }
    if let Some(group_list) = &pack_config.groups {
        let config = tree_sitter_language_pack::PackConfig {
            cache_dir: effective_cache_dir.cloned(),
            languages: None,
            groups: Some(group_list.clone()),
        };
        tree_sitter_language_pack::init(&config).context("Failed to download tree-sitter grammar groups")?;
        return Ok(DownloadOutcome {
            count: 0,
            description: format!("groups: {}", group_list.join(", ")),
        });
    }
    if let Some(langs) = &pack_config.languages {
        let refs: Vec<&str> = langs.iter().map(String::as_str).collect();
        let count = tree_sitter_language_pack::download(&refs).context("Failed to download tree-sitter grammars")?;
        return Ok(DownloadOutcome {
            count,
            description: format!("languages: {}", langs.join(", ")),
        });
    }
    anyhow::bail!(
        "No languages specified. Use language names, --all, --groups, or --from-config \
         (with tree_sitter.languages/groups set in the xberg config file)."
    );
}

/// Print the download outcome in the requested wire format.
#[expect(
    clippy::print_stdout,
    reason = "tree-sitter download summary is the command's stdout result output"
)]
fn print_download_summary(
    format: WireFormat,
    pack_config: &tree_sitter_language_pack::PackConfig,
    all: bool,
    outcome: &DownloadOutcome,
    effective_cache_dir: Option<&PathBuf>,
) -> Result<()> {
    match format {
        WireFormat::Text => {
            println!("{}", style::header("Tree-sitter Download"));
            println!("{}", style::dim("===================="));
            println!("{} {}", style::label("Requested:"), outcome.description);
            if pack_config.groups.is_none() || all || pack_config.languages.is_some() {
                println!(
                    "{} {}",
                    style::label("Newly downloaded:"),
                    style::success(&outcome.count.to_string())
                );
            }
            if let Some(dir) = effective_cache_dir {
                println!(
                    "{} {}",
                    style::label("Cache directory:"),
                    style::success(&dir.display().to_string())
                );
            }
            println!("{}", style::success("Done"));
        }
        WireFormat::Json => {
            let mut output = json!({
                "requested": outcome.description,
                "newly_downloaded": outcome.count,
            });
            if let Some(dir) = effective_cache_dir {
                output["cache_dir"] = json!(dir.to_string_lossy());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&output).context("Failed to serialize download results to JSON")?
            );
        }
        WireFormat::Toon => {
            let mut output = json!({
                "requested": outcome.description,
                "newly_downloaded": outcome.count,
            });
            if let Some(dir) = effective_cache_dir {
                output["cache_dir"] = json!(dir.to_string_lossy());
            }
            println!(
                "{}",
                serde_toon::to_string(&output).context("Failed to serialize download results to TOON")?
            );
        }
    }

    Ok(())
}

/// Execute the tree-sitter download command.
///
/// Downloads tree-sitter grammar parsers based on the provided arguments:
/// - Specific languages by name
/// - All available languages (--all)
/// - Language groups (--groups)
/// - The auto-discovered/`--config` xberg config's `[tree_sitter]` section
///   (--from-config), for `cache_dir`/`languages`/`groups`
pub fn download_command(
    languages: Vec<String>,
    all: bool,
    groups: Option<Vec<String>>,
    cache_dir: Option<PathBuf>,
    from_config: bool,
    format: WireFormat,
) -> Result<()> {
    let file_config = if from_config {
        Some(
            load_config(None, true)
                .context("Failed to load xberg configuration for --from-config")?
                .tree_sitter
                .unwrap_or_default(),
        )
    } else {
        None
    };

    let pack_config = resolve_pack_config(cache_dir.clone(), &languages, groups.as_deref(), file_config.as_ref());
    let effective_cache_dir = pack_config.cache_dir.clone();

    configure_pack_cache_dir(effective_cache_dir.as_ref())?;

    let outcome = download_selected_grammars(&pack_config, all, effective_cache_dir.as_ref())?;

    print_download_summary(format, &pack_config, all, &outcome, effective_cache_dir.as_ref())
}

/// Execute the tree-sitter list command.
///
/// Lists available or downloaded tree-sitter languages, optionally filtering
/// by a name substring.
#[expect(
    clippy::print_stdout,
    reason = "tree-sitter language list is the command's stdout result output"
)]
pub fn list_command(downloaded_only: bool, filter: Option<String>, format: WireFormat) -> Result<()> {
    let languages = if downloaded_only {
        tree_sitter_language_pack::downloaded_languages()
    } else {
        tree_sitter_language_pack::manifest_languages().context("Failed to fetch tree-sitter language manifest")?
    };

    let filtered: Vec<&String> = if let Some(ref f) = filter {
        let lower = f.to_lowercase();
        languages.iter().filter(|l| l.to_lowercase().contains(&lower)).collect()
    } else {
        languages.iter().collect()
    };

    let source = if downloaded_only { "downloaded" } else { "available" };

    match format {
        WireFormat::Text => {
            println!(
                "{} ({} {}{})",
                style::header("Tree-sitter Languages"),
                filtered.len(),
                source,
                filter.as_ref().map(|f| format!(", filter: '{f}'")).unwrap_or_default()
            );
            println!("{}", style::dim("====================="));
            for lang in &filtered {
                println!("  {}", style::success(lang));
            }
        }
        WireFormat::Json => {
            let output = json!({
                "source": source,
                "count": filtered.len(),
                "filter": filter,
                "languages": filtered,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).context("Failed to serialize language list to JSON")?
            );
        }
        WireFormat::Toon => {
            let output = json!({
                "source": source,
                "count": filtered.len(),
                "filter": filter,
                "languages": filtered,
            });
            println!(
                "{}",
                serde_toon::to_string(&output).context("Failed to serialize language list to TOON")?
            );
        }
    }

    Ok(())
}

/// Execute the tree-sitter cache-dir command.
///
/// Displays the effective cache directory for tree-sitter grammar parsers.
#[expect(
    clippy::print_stdout,
    reason = "tree-sitter cache directory is the command's stdout result output"
)]
pub fn cache_dir_command(format: WireFormat) -> Result<()> {
    let dir = tree_sitter_language_pack::cache_dir().context("Failed to determine tree-sitter cache directory")?;
    // `cache_dir()` already yields a String, not a PathBuf.
    let dir_str = dir;

    match format {
        WireFormat::Text => {
            println!("{} {}", style::label("Cache directory:"), style::success(&dir_str));
        }
        WireFormat::Json => {
            let output = json!({ "cache_dir": dir_str });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).context("Failed to serialize cache directory to JSON")?
            );
        }
        WireFormat::Toon => {
            let output = json!({ "cache_dir": dir_str });
            println!(
                "{}",
                serde_toon::to_string(&output).context("Failed to serialize cache directory to TOON")?
            );
        }
    }

    Ok(())
}

/// Execute the tree-sitter clean command.
///
/// Clears all cached tree-sitter grammar parser shared libraries.
#[expect(
    clippy::print_stdout,
    reason = "tree-sitter clean status is the command's stdout result output"
)]
pub fn clean_command(format: WireFormat) -> Result<()> {
    tree_sitter_language_pack::clean_cache().context("Failed to clean tree-sitter cache")?;

    match format {
        WireFormat::Text => {
            println!("{}", style::success("Tree-sitter cache cleared successfully"));
        }
        WireFormat::Json => {
            let output = json!({ "status": "cleared" });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).context("Failed to serialize clean result to JSON")?
            );
        }
        WireFormat::Toon => {
            let output = json!({ "status": "cleared" });
            println!(
                "{}",
                serde_toon::to_string(&output).context("Failed to serialize clean result to TOON")?
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no CLI overrides, `resolve_pack_config` must carry the
    /// `--from-config`-loaded `TreeSitterConfig`'s `cache_dir`/`languages`/`groups`
    /// through into the `PackConfig` unchanged — this is the "three fields
    /// finally have a reader" behavior the flag exists to provide.
    #[test]
    fn should_carry_file_config_fields_into_pack_config_when_no_cli_args_given() {
        let file_config = xberg::TreeSitterConfig {
            cache_dir: Some(PathBuf::from("/var/cache/xberg-grammars")),
            languages: Some(vec!["python".to_string(), "rust".to_string()]),
            groups: Some(vec!["web".to_string()]),
            ..Default::default()
        };

        let pack_config = resolve_pack_config(None, &[], None, Some(&file_config));

        assert_eq!(pack_config.cache_dir, Some(PathBuf::from("/var/cache/xberg-grammars")));
        assert_eq!(
            pack_config.languages,
            Some(vec!["python".to_string(), "rust".to_string()])
        );
        assert_eq!(pack_config.groups, Some(vec!["web".to_string()]));
    }

    /// An explicit `--cache-dir` CLI argument must win over the config file's
    /// `cache_dir`, per the documented cascade (CLI args > config file).
    #[test]
    fn should_override_file_config_cache_dir_with_explicit_cli_cache_dir() {
        let file_config = xberg::TreeSitterConfig {
            cache_dir: Some(PathBuf::from("/from/config")),
            ..Default::default()
        };

        let pack_config = resolve_pack_config(Some(PathBuf::from("/from/cli")), &[], None, Some(&file_config));

        assert_eq!(pack_config.cache_dir, Some(PathBuf::from("/from/cli")));
    }

    /// Explicit CLI languages must win over the config file's `languages`,
    /// not merge with them.
    #[test]
    fn should_override_file_config_languages_with_explicit_cli_languages() {
        let file_config = xberg::TreeSitterConfig {
            languages: Some(vec!["python".to_string()]),
            ..Default::default()
        };
        let cli_languages = vec!["go".to_string(), "zig".to_string()];

        let pack_config = resolve_pack_config(None, &cli_languages, None, Some(&file_config));

        assert_eq!(pack_config.languages, Some(vec!["go".to_string(), "zig".to_string()]));
    }

    /// Explicit CLI groups must win over the config file's `groups`, not
    /// merge with them.
    #[test]
    fn should_override_file_config_groups_with_explicit_cli_groups() {
        let file_config = xberg::TreeSitterConfig {
            groups: Some(vec!["web".to_string()]),
            ..Default::default()
        };
        let cli_groups = vec!["systems".to_string()];

        let pack_config = resolve_pack_config(None, &[], Some(&cli_groups), Some(&file_config));

        assert_eq!(pack_config.groups, Some(vec!["systems".to_string()]));
    }

    /// With no CLI args and no config file loaded (`--from-config` not
    /// passed), every `PackConfig` field must resolve to `None` — TSLP falls
    /// back to its own defaults.
    #[test]
    fn should_resolve_all_none_when_no_cli_args_and_no_file_config() {
        let pack_config = resolve_pack_config(None, &[], None, None);

        assert_eq!(pack_config.cache_dir, None);
        assert_eq!(pack_config.languages, None);
        assert_eq!(pack_config.groups, None);
    }
}
