#!/usr/bin/env python3
"""Emit the ground-truth language sidecar (`ground_truth/languages.json`).

Part of issue #1696's corpus work: `plausibility_calibrate.py`'s false-positive cohorts (and any
future per-language calibration) need to know which ground-truth documents are genuinely
non-English, rather than inferring it from filenames. This script is the generator; the schema
below is the single source of truth `build_corpus.py` reads it back through.

For every `<id>.txt` under a ground-truth directory, this shells out to a release build of the
`xberg` CLI (`cargo build -p xberg-cli --release --features pdf-ocr,analysis` -- `analysis`
carries `language-detection`, the feature `--detect-language` needs) with `--detect-language
true --config-json '{"language_detection":{"min_confidence":0.9}}'`, and reads the first entry of
`result.detected_languages`.

NOT `result.detected_language_confidences[0].reliable`, despite that field existing on
`ExtractedDocument` with exactly the right shape (`language`/`confidence`/`reliable`): measured
against the CLI's actual JSON output, that field comes back `null` even when `detected_languages`
is populated. `execute_language_detection` (`core/pipeline/features.rs`), the code path
`--detect-language` drives directly, only ever writes `result.detected_languages`; the
`LanguageDetector` `PostProcessor` plugin (`language_detection/processor.rs`) is the only site
that populates `detected_language_confidences`, and it is not what the plain CLI flag invokes.
Filed as a follow-up defect, not fixed here. Given that gap, this script instead raises
`language_detection.min_confidence` to 0.9 via `--config-json` -- the same bar
`LanguageConfidence::reliable` documents -- so a non-empty `detected_languages` already means
"reliable" without needing the missing confidence field at all; the sidecar records that first
ISO 639-3 code, or `"und"` (ISO 639-2 "undetermined") when the list is empty, rather than guessing.

Output schema (`ground_truth/languages.json`):

    {"schema": 1, "languages": {"<id>": "eng" | "und" | ...}}

Usage:
    build_language_sidecar.py --binary target/release/xberg <gt_dir> [<gt_dir> ...]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SIDECAR_SCHEMA_VERSION = 1
UNDETERMINED_LANGUAGE = "und"
EXTRACT_TIMEOUT_SECONDS = 60
# Matches `LanguageConfidence::reliable`'s own bar (whatlang confidence >= 0.9), applied via
# `--config-json` since the CLI's `--detect-language` flag alone uses the 0.8 pipeline default
# (see this module's docstring for why `detected_languages` is read instead of a per-language
# confidence field). ~keep
RELIABLE_MIN_CONFIDENCE_CONFIG_JSON = '{"language_detection":{"min_confidence":0.9}}'


def detect_language(binary: Path, txt_path: Path, cache_dir: Path) -> str:
    """Run one `xberg extract --detect-language true` call at the 0.9 reliability bar.

    Returns `UNDETERMINED_LANGUAGE` on any failure, empty result, or no language clearing 0.9
    confidence.
    """
    env = dict(os.environ)
    env["XBERG_CACHE_DIR"] = str(cache_dir)
    cmd = [
        str(binary),
        "extract",
        str(txt_path),
        "--format",
        "json",
        "--no-config-discovery",
        "--no-cache",
        "true",
        "--detect-language",
        "true",
        "--config-json",
        RELIABLE_MIN_CONFIDENCE_CONFIG_JSON,
    ]
    try:
        proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=EXTRACT_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        print(f"  [error] {txt_path.name}: language detection timed out", file=sys.stderr)
        return UNDETERMINED_LANGUAGE
    if proc.returncode != 0:
        print(f"  [error] {txt_path.name}: exit {proc.returncode}: {proc.stderr.strip()[:200]}", file=sys.stderr)
        return UNDETERMINED_LANGUAGE
    try:
        envelope = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        print(f"  [error] {txt_path.name}: could not parse JSON output: {exc}", file=sys.stderr)
        return UNDETERMINED_LANGUAGE

    languages = envelope.get("result", {}).get("detected_languages")
    if not languages:
        return UNDETERMINED_LANGUAGE
    language = languages[0]
    return language if isinstance(language, str) and language else UNDETERMINED_LANGUAGE


def build_sidecar(binary: Path, gt_dirs: list[Path], cache_dir: Path) -> dict[str, str]:
    languages: dict[str, str] = {}
    for gt_dir in gt_dirs:
        for txt_path in sorted(gt_dir.glob("*.txt")):
            doc_id = txt_path.stem
            languages[doc_id] = detect_language(binary, txt_path, cache_dir)
            print(f"  {doc_id}: {languages[doc_id]}")
    return languages


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("gt_dirs", nargs="+", help="ground-truth directories to scan for *.txt files")
    parser.add_argument("--binary", default="target/release/xberg", help="path to the release xberg CLI binary")
    parser.add_argument(
        "--out",
        default="test_documents/ground_truth/languages.json",
        help="path to write the sidecar JSON to",
    )
    args = parser.parse_args(argv)

    binary = Path(args.binary).resolve()
    if not binary.is_file():
        parser.error(
            f"CLI binary not found at {binary}; build it first with "
            "`cargo build -p xberg-cli --release --features pdf-ocr,analysis`"
        )

    gt_dirs = [Path(d).resolve() for d in args.gt_dirs]
    missing = [str(d) for d in gt_dirs if not d.is_dir()]
    if missing:
        parser.error(f"ground-truth directory not found: {', '.join(missing)}")

    cache_dir = Path(tempfile.mkdtemp(prefix="xberg-language-sidecar-"))
    if not str(cache_dir) or not cache_dir.is_dir():
        parser.error("failed to create an isolated XBERG_CACHE_DIR; refusing to run uncached")

    try:
        languages = build_sidecar(binary, gt_dirs, cache_dir)
    finally:
        shutil.rmtree(cache_dir, ignore_errors=True)

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    sidecar = {"schema": SIDECAR_SCHEMA_VERSION, "languages": dict(sorted(languages.items()))}
    out_path.write_text(json.dumps(sidecar, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out_path} ({len(languages)} document(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
