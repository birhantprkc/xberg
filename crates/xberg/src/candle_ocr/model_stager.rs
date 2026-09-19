//! Auto-download and checksum-verify local weights for the Candle VLM-OCR backends.
//!
//! PaddleOCR-VL 1.6 stays up on the Hub (`PaddlePaddle/PaddleOCR-VL-1.6`), but this
//! module re-hosts it under `xberg-io/paddleocr-vl-1.6` and fetches it through the
//! normal `hf-hub` path at an immutable revision so the backend's default `model_id` auto-downloads without
//! requiring callers to pre-stage a local `model_path`, verifying every file against a
//! checked-in sha256 manifest before use.
//!
//! DeepSeek-OCR is fetched directly from the original `deepseek-ai/DeepSeek-OCR` repository
//! (no re-hosted mirror) at a pinned revision, with the same checksum-manifest trust model.
//!
//! Trust attaches to the manifest, not the host: a changed or tampered file fails
//! the staging step instead of silently feeding wrong weights into inference. The
//! shards are byte-identical to the original release, so the checked-in checksums are
//! unchanged. Caching is handled by `hf-hub` (the shared blob cache), so weights are
//! fetched once and reused across runs.

use std::path::{Path, PathBuf};

use crate::model_download::parse_sha256_manifest;
#[cfg(test)]
use crate::model_download::verify_sha256;

/// A checksum-pinned VLM-OCR model hosted on the Hugging Face Hub.
struct HfModel {
    /// Hugging Face repo id, e.g. `xberg-io/paddleocr-vl-1.6`.
    repo: &'static str,
    /// Immutable Hub commit containing the manifest-pinned files.
    revision: &'static str,
    /// `sha256sum`-format manifest: `<sha256>  <filename>` per line, `#` comments.
    /// One entry per file the engine reads, checked in as the single source of truth.
    manifest: &'static str,
}

/// SHA-256 manifest pinning every hosted PaddleOCR-VL 1.6 file, checked in as the single
/// source of truth. Trust attaches to the manifest, not the host — a changed or tampered
/// upstream file fails staging instead of feeding wrong weights into inference.
#[cfg(feature = "candle-paddleocr-vl")]
pub(crate) const PADDLEOCR_VL_16_SHA256_MANIFEST: &str = include_str!("paddleocr-vl-1.6.sha256");

/// PaddleOCR-VL 1.6 — re-hosted at `xberg-io/paddleocr-vl-1.6` (byte-identical mirror
/// of `PaddlePaddle/PaddleOCR-VL-1.6`, Apache-2.0). The upstream repo is public, but we
/// mirror it anyway so the backend's default `model_id` resolves to a checksum-pinned
/// copy under our control rather than trusting whatever the upstream repo currently
/// contains at fetch time.
#[cfg(feature = "candle-paddleocr-vl")]
const PADDLEOCR_VL_16: HfModel = HfModel {
    repo: "xberg-io/paddleocr-vl-1.6",
    revision: "2ce18e126792e5e5e806553ff4fa84f3684c2c34",
    manifest: PADDLEOCR_VL_16_SHA256_MANIFEST,
};

/// Parse the model manifest into an ordered `(filename, sha256)` list, requiring at
/// least one entry. Format/validation live in [`parse_sha256_manifest`], shared with
/// the other checksum-manifest consumers.
fn manifest_files(content: &str) -> Result<Vec<(String, String)>, String> {
    let files = parse_sha256_manifest(content)?;
    if files.is_empty() {
        return Err("Manifest lists no files".to_string());
    }
    Ok(files)
}

/// Ensure PaddleOCR-VL 1.6 weights are present locally and return the model directory.
///
/// `repo_id` is normally the backend's default `xberg-io/paddleocr-vl-1.6` — in that
/// case every manifest file is fetched from an immutable revision through `hf-hub`
/// (warm cache hits skip the
/// network) and verified against the checked-in sha256 manifest before use, so a
/// tampered or corrupted download fails staging instead of silently feeding wrong
/// weights into inference.
///
/// A caller-supplied `repo_id` (via `backend_options.model_id`) that does not match the
/// pinned mirror has no corresponding checksum manifest, so it is fetched via plain
/// `hf-hub` without checksum verification — the same trust level as pointing
/// `model_path` at arbitrary local weights.
#[cfg(feature = "candle-paddleocr-vl")]
pub(crate) fn ensure_paddleocr_vl_16(
    repo_id: &str,
    revision: Option<&str>,
    cache_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if repo_id == PADDLEOCR_VL_16.repo {
        return ensure_model(&PADDLEOCR_VL_16, revision, cache_dir);
    }

    tracing::warn!(
        repo = repo_id,
        pinned_repo = PADDLEOCR_VL_16.repo,
        "PaddleOCR-VL model_id does not match the checksum-pinned mirror; downloading without \
         checksum verification"
    );
    let revision = revision.ok_or_else(|| {
        format!(
            "custom PaddleOCR-VL model '{repo_id}' requires an explicit immutable `hf_revision`; refusing to resolve a mutable default branch"
        )
    })?;
    ensure_model_unverified(repo_id, revision, cache_dir, PADDLEOCR_VL_16_FILES)
}

/// Fetch every file in `files` from `repo_id` via `hf-hub` without checksum
/// verification, returning the shared snapshot directory. Used only for a
/// non-default `model_id` override where no checked-in manifest exists.
#[cfg(feature = "candle-paddleocr-vl")]
fn ensure_model_unverified(
    repo_id: &str,
    revision: &str,
    cache_dir: Option<&Path>,
    files: &[&str],
) -> Result<PathBuf, String> {
    let mut dir: Option<PathBuf> = None;
    for name in files {
        let path = crate::model_download::hf_resolve_file(repo_id, name, Some(revision), cache_dir, None)?;
        if dir.is_none() {
            dir = path.parent().map(Path::to_path_buf);
        }
    }
    dir.ok_or_else(|| format!("Fetched no files for {repo_id}"))
}

/// Filenames the PaddleOCR-VL engine and processor read, used to fetch a
/// non-default `model_id` that has no checksum manifest of its own.
#[cfg(feature = "candle-paddleocr-vl")]
const PADDLEOCR_VL_16_FILES: &[&str] = &[
    "config.json",
    "preprocessor_config.json",
    "tokenizer.json",
    "model.safetensors",
];

/// SHA-256 manifest pinning every file `DeepseekOCREngine::init` reads at the pinned
/// revision, checked in as the single source of truth. Trust attaches to the manifest,
/// not the host -- a changed or tampered upstream file fails staging instead of feeding
/// wrong weights into inference.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
pub(crate) const DEEPSEEK_OCR_SHA256_MANIFEST: &str = include_str!("deepseek-ocr.sha256");

/// DeepSeek-OCR, fetched directly from the original `deepseek-ai/DeepSeek-OCR` repository
/// (unlike PaddleOCR-VL 1.6 above, there is no re-hosted mirror): the upstream repo is the
/// only source, so pinning the revision plus checksums is the whole trust boundary.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
const DEEPSEEK_OCR: HfModel = HfModel {
    repo: "deepseek-ai/DeepSeek-OCR",
    revision: "9f30c71f441d010e5429c532364a86705536c53a",
    manifest: DEEPSEEK_OCR_SHA256_MANIFEST,
};

/// Ensure DeepSeek-OCR weights are present locally and return the model directory.
///
/// `repo_id` is normally the backend's default `deepseek-ai/DeepSeek-OCR` -- in that case
/// every manifest file is fetched from the pinned revision through `hf-hub` (warm cache hits
/// skip the network) and verified against the checked-in sha256 manifest before use, so a
/// tampered or corrupted download fails staging instead of silently feeding wrong weights
/// into inference.
///
/// A caller-supplied `repo_id` (via `backend_options.model_id`) that does not match the
/// pinned repository has no corresponding checksum manifest, so its `config.json`,
/// `tokenizer.json`, and every shard named by its own `model.safetensors.index.json` are
/// fetched via plain `hf-hub` without checksum verification -- the same trust level as
/// pointing `model_path` at arbitrary local weights.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
pub(crate) fn ensure_deepseek_ocr(
    repo_id: &str,
    revision: Option<&str>,
    cache_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if repo_id == DEEPSEEK_OCR.repo {
        return ensure_model(&DEEPSEEK_OCR, revision, cache_dir);
    }

    tracing::warn!(
        repo = repo_id,
        pinned_repo = DEEPSEEK_OCR.repo,
        "DeepSeek-OCR model_id does not match the checksum-pinned repository; downloading \
         without checksum verification"
    );
    let revision = revision.ok_or_else(|| {
        format!(
            "custom DeepSeek-OCR model '{repo_id}' requires an explicit immutable `hf_revision`; refusing to resolve a mutable default branch"
        )
    })?;
    ensure_deepseek_ocr_unverified(repo_id, revision, cache_dir)
}

/// Fetch DeepSeek-OCR's config, tokenizer, and every shard named by its own
/// `model.safetensors.index.json` from `repo_id` via `hf-hub` without checksum
/// verification. Used only for a non-default `model_id` override where no checked-in
/// manifest exists; the shard set is read from the index rather than assumed, so this
/// keeps working if a future custom repo shards its weights differently than the pinned
/// default.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
fn ensure_deepseek_ocr_unverified(repo_id: &str, revision: &str, cache_dir: Option<&Path>) -> Result<PathBuf, String> {
    let mut dir: Option<PathBuf> = None;
    for name in ["config.json", "tokenizer.json"] {
        let path = crate::model_download::hf_resolve_file(repo_id, name, Some(revision), cache_dir, None)?;
        if dir.is_none() {
            dir = path.parent().map(Path::to_path_buf);
        }
    }

    let index_path = crate::model_download::hf_resolve_file(
        repo_id,
        "model.safetensors.index.json",
        Some(revision),
        cache_dir,
        None,
    )?;
    let index_str = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("Failed to read fetched safetensors index for {repo_id}: {e}"))?;
    for shard in index_shard_files(&index_str)? {
        crate::model_download::hf_resolve_file(repo_id, &shard, Some(revision), cache_dir, None)?;
    }

    dir.ok_or_else(|| format!("Fetched no files for {repo_id}"))
}

/// Parse a `model.safetensors.index.json` document's `weight_map` into the deduplicated,
/// sorted set of shard filenames it names.
#[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
fn index_shard_files(index_json: &str) -> Result<Vec<String>, String> {
    let index: serde_json::Value =
        serde_json::from_str(index_json).map_err(|e| format!("Failed to parse safetensors index: {e}"))?;
    let weight_map = index
        .get("weight_map")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "safetensors index has no weight_map object".to_string())?;

    let mut files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for value in weight_map.values() {
        if let Some(name) = value.as_str() {
            files.insert(name.to_string());
        }
    }
    if files.is_empty() {
        return Err("safetensors index weight_map names no files".to_string());
    }
    Ok(files.into_iter().collect())
}

fn ensure_model(model: &HfModel, revision: Option<&str>, cache_dir: Option<&Path>) -> Result<PathBuf, String> {
    let files = manifest_files(model.manifest)?;
    let revision = revision.unwrap_or(model.revision);

    let mut dir: Option<PathBuf> = None;
    for (name, sha256) in &files {
        let path = crate::model_download::hf_resolve_file(model.repo, name, Some(revision), cache_dir, Some(sha256))?;
        if dir.is_none() {
            dir = path.parent().map(Path::to_path_buf);
        }
    }

    dir.ok_or_else(|| format!("Fetched no files for {}", model.repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "candle-paddleocr-vl")]
    #[test]
    fn paddleocr_vl_16_manifest_covers_every_file_the_engine_reads() {
        let files = manifest_files(PADDLEOCR_VL_16.manifest).expect("bundled manifest must parse");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        for required in [
            "config.json",
            "preprocessor_config.json",
            "tokenizer.json",
            "model.safetensors",
        ] {
            assert!(names.contains(&required), "manifest missing {required}");
        }
    }

    #[cfg(feature = "candle-paddleocr-vl")]
    #[test]
    fn paddleocr_vl_16_is_hosted_on_the_xberg_hf_repo() {
        assert_eq!(PADDLEOCR_VL_16.repo, "xberg-io/paddleocr-vl-1.6");
        assert_eq!(PADDLEOCR_VL_16.revision.len(), 40);
        assert!(PADDLEOCR_VL_16.revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn manifest_files_requires_at_least_one_entry() {
        assert!(manifest_files("# only comments\n").is_err(), "no files");
        assert_eq!(
            manifest_files(&format!("{}  config.json\n", "a".repeat(64)))
                .unwrap()
                .len(),
            1
        );
    }

    /// End-to-end check of the real hf-hub → verify path against the re-hosted
    /// `xberg-io/paddleocr-vl-1.6` mirror, using only the small config/tokenizer files
    /// (no ~2 GB safetensors shard). Ignored by default (network); run with `--ignored`.
    #[cfg(feature = "candle-paddleocr-vl")]
    #[test]
    #[ignore = "hits the HuggingFace Hub; run with --ignored"]
    fn stages_paddleocr_vl_16_config_files_from_hf() {
        let bundled = manifest_files(PADDLEOCR_VL_16.manifest).unwrap();
        let small: Vec<(String, String)> = bundled
            .into_iter()
            .filter(|(name, _)| name != "model.safetensors")
            .collect();
        assert!(
            !small.is_empty(),
            "manifest should list small config/tokenizer files besides the weights"
        );
        let manifest: &'static str = Box::leak(
            small
                .iter()
                .map(|(name, sha256)| format!("{sha256}  {name}"))
                .collect::<Vec<_>>()
                .join("\n")
                .into_boxed_str(),
        );
        let model = HfModel {
            repo: PADDLEOCR_VL_16.repo,
            revision: PADDLEOCR_VL_16.revision,
            manifest,
        };

        let out = ensure_model(&model, None, None).expect("staging must succeed");
        for (name, sha256) in &small {
            let path = out.join(name);
            assert!(path.exists(), "{name} should be staged in the snapshot dir");
            verify_sha256(&path, sha256, name).expect("staged file must match manifest checksum");
        }

        ensure_model(&model, None, None).expect("warm cache must succeed");
    }

    #[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
    #[test]
    fn deepseek_ocr_manifest_covers_every_file_the_engine_reads() {
        let files = manifest_files(DEEPSEEK_OCR.manifest).expect("bundled manifest must parse");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        for required in [
            "config.json",
            "tokenizer.json",
            "model.safetensors.index.json",
            "model-00001-of-000001.safetensors",
        ] {
            assert!(names.contains(&required), "manifest missing {required}");
        }
    }

    #[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
    #[test]
    fn deepseek_ocr_is_pinned_to_the_upstream_hf_repo() {
        assert_eq!(DEEPSEEK_OCR.repo, "deepseek-ai/DeepSeek-OCR");
        assert_eq!(DEEPSEEK_OCR.revision.len(), 40);
        assert!(DEEPSEEK_OCR.revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
    #[test]
    fn index_shard_files_extracts_deduplicated_sorted_shard_names() {
        let index =
            r#"{"weight_map": {"a": "shard-2.safetensors", "b": "shard-1.safetensors", "c": "shard-2.safetensors"}}"#;
        assert_eq!(
            index_shard_files(index).unwrap(),
            vec!["shard-1.safetensors".to_string(), "shard-2.safetensors".to_string()]
        );
    }

    #[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
    #[test]
    fn index_shard_files_rejects_a_missing_weight_map() {
        assert!(index_shard_files("{}").is_err());
        assert!(index_shard_files(r#"{"weight_map": {}}"#).is_err());
    }

    /// End-to-end check of the real hf-hub → verify path against the pinned
    /// `deepseek-ai/DeepSeek-OCR` revision, using only the small config/tokenizer/index files
    /// (no ~6.7 GB safetensors shard). Ignored by default (network); run with `--ignored`.
    #[cfg(all(feature = "candle-deepseek-ocr", not(target_arch = "wasm32")))]
    #[test]
    #[ignore = "hits the HuggingFace Hub; run with --ignored"]
    fn stages_deepseek_ocr_small_files_from_hf() {
        let bundled = manifest_files(DEEPSEEK_OCR.manifest).unwrap();
        let small: Vec<(String, String)> = bundled
            .into_iter()
            .filter(|(name, _)| !name.ends_with(".safetensors"))
            .collect();
        assert!(
            !small.is_empty(),
            "manifest should list small config/tokenizer/index files besides the weights"
        );
        let manifest: &'static str = Box::leak(
            small
                .iter()
                .map(|(name, sha256)| format!("{sha256}  {name}"))
                .collect::<Vec<_>>()
                .join("\n")
                .into_boxed_str(),
        );
        let model = HfModel {
            repo: DEEPSEEK_OCR.repo,
            revision: DEEPSEEK_OCR.revision,
            manifest,
        };

        let out = ensure_model(&model, None, None).expect("staging must succeed");
        for (name, sha256) in &small {
            let path = out.join(name);
            assert!(path.exists(), "{name} should be staged in the snapshot dir");
            verify_sha256(&path, sha256, name).expect("staged file must match manifest checksum");
        }

        ensure_model(&model, None, None).expect("warm cache must succeed");
    }
}
