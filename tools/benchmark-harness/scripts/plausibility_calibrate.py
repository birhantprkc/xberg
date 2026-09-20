#!/usr/bin/env python3
"""Calibrate issue #1696's language/dictionary-plausibility OCR-routing signal.

Shells out to a release build of the `xberg` CLI (`cargo build -p xberg-cli --release
--features pdf-ocr`) and reads `result.metadata.format.implausible_text_pages` from its JSON
output for two populations:

* Six false-positive cohorts, sourced from `tools/benchmark-harness/fixtures/pdf/*.json`'s
  `metadata.cohorts` tags: `native-clean`, `tables`, `complex-span-table`, `wide-table`,
  `formula`, `code`. A document counts as a false positive when it has one or more flagged
  pages.
* The `wrong_mapping` recall cohort, sourced directly from `test_documents/pdf/wrong_mapping/`
  (not yet wired into `tools/benchmark-harness/fixtures/`, per the corpus work tracked
  separately from this script). A document counts as recalled when at least one of its pages
  is flagged -- NOT when every page is, which real corpus measurement showed is not always
  achievable (a short tail page can legitimately fall below the minimum prose floor and
  abstain rather than guess; see `evaluate_wrong_mapping_recall`'s own docstring). Per-document
  full-page coverage is still reported (`fully_flagged`) for visibility, not as the target.

Acceptance (design doc, not CI-gated): false-positive rate < 0.5% on `native-clean`, < 2% on
each other cohort, 100% recall (any page flagged) on `wrong_mapping`. Exits non-zero when a
printed number misses its target so the acceptance check cannot be misread from a truncated
run, but nothing in CI invokes this script -- see the design doc's own "harness not CI-gated"
note.

Cache isolation follows this repo's `extraction-measurement-hygiene` rule: a single
`tempfile.mkdtemp` directory is created for the run, verified non-empty, exported as
`XBERG_CACHE_DIR` for every subprocess call, and removed afterward. An unset/empty
`XBERG_CACHE_DIR` would silently fall back to the shared user cache and invalidate every
number this script prints.
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

# The six false-positive cohorts, exactly as tagged in `metadata.cohorts` on fixture JSON
# files under `tools/benchmark-harness/fixtures/pdf/`. ~keep
FALSE_POSITIVE_COHORTS: tuple[str, ...] = (
    "native-clean",
    "tables",
    "complex-span-table",
    "wide-table",
    "formula",
    "code",
)
# `native-clean` gets the tighter ceiling: it is real, correctly-mapped prose, so any flag on
# it is a pure false positive. The other cohorts are structurally non-prose-heavy (tables,
# formulas, code) and get more headroom because their few genuine prose lines are a smaller,
# noisier sample. ~keep
FALSE_POSITIVE_CEILING_NATIVE_CLEAN = 0.005
FALSE_POSITIVE_CEILING_OTHER = 0.02
WRONG_MAPPING_RECALL_TARGET = 1.0
EXTRACT_TIMEOUT_SECONDS = 300


def load_fixture_cohorts(fixtures_dir: Path) -> dict[str, list[Path]]:
    """Map each false-positive cohort name to the resolved PDF paths tagged with it."""
    cohort_paths: dict[str, list[Path]] = {cohort: [] for cohort in FALSE_POSITIVE_COHORTS}
    for fixture_path in sorted(fixtures_dir.glob("*.json")):
        try:
            data = json.loads(fixture_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        document = data.get("document")
        if not document:
            continue
        doc_path = (fixtures_dir / document).resolve()
        for cohort in data.get("metadata", {}).get("cohorts", []):
            if cohort in cohort_paths:
                cohort_paths[cohort].append(doc_path)
    return cohort_paths


def run_extract(binary: Path, pdf_path: Path, cache_dir: Path) -> dict | None:
    """Run one `xberg extract --format json` call under an isolated cache; `None` on failure."""
    env = dict(os.environ)
    env["XBERG_CACHE_DIR"] = str(cache_dir)
    cmd = [
        str(binary),
        "extract",
        str(pdf_path),
        "--format",
        "json",
        "--no-config-discovery",
        "--no-cache",
        "true",
        "--ocr-no-cache",
        "true",
    ]
    try:
        proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=EXTRACT_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        print(f"  [error] {pdf_path.name}: extraction timed out after {EXTRACT_TIMEOUT_SECONDS}s", file=sys.stderr)
        return None
    if proc.returncode != 0:
        print(f"  [error] {pdf_path.name}: exit {proc.returncode}: {proc.stderr.strip()[:200]}", file=sys.stderr)
        return None
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        print(f"  [error] {pdf_path.name}: could not parse JSON output: {exc}", file=sys.stderr)
        return None


def implausible_pages(envelope: dict) -> list[int] | None:
    """Read `result.metadata.format.implausible_text_pages`; `None` when absent/not a PDF."""
    result = envelope.get("result", {})
    fmt = result.get("metadata", {}).get("format")
    if not isinstance(fmt, dict) or fmt.get("format_type") != "pdf":
        return None
    return fmt.get("implausible_text_pages")


def page_count(envelope: dict) -> int:
    return envelope.get("result", {}).get("counts", {}).get("pages", 0)


def evaluate_false_positive_cohort(binary: Path, paths: list[Path], cache_dir: Path) -> dict:
    """A document counts as a false positive when `implausible_text_pages` is non-empty."""
    evaluated = 0
    missing = 0
    flagged_ids: list[str] = []
    for path in paths:
        if not path.exists():
            missing += 1
            continue
        envelope = run_extract(binary, path, cache_dir)
        if envelope is None:
            missing += 1
            continue
        evaluated += 1
        pages = implausible_pages(envelope)
        if pages:
            flagged_ids.append(path.stem)
    fp_rate = len(flagged_ids) / evaluated if evaluated else 0.0
    return {"evaluated": evaluated, "missing": missing, "flagged_ids": flagged_ids, "fp_rate": fp_rate}


def evaluate_wrong_mapping_recall(binary: Path, paths: list[Path], cache_dir: Path) -> dict:
    """Recall against the `wrong_mapping` corpus.

    Acceptance uses "at least one page flagged" per document, not "every page flagged" -- measured
    against the real corpus, not assumed: a short tail page (few hundred prose characters after
    excluding bulleted lines) legitimately falls below `PLAUSIBILITY_MIN_PROSE_CHUNKS` and abstains
    (`PlausibilityVerdict::NotEvaluated`) rather than guessing, so full-page coverage is not always
    achievable even on a genuinely wrong-mapped document. `fully_flagged` is still reported per
    document for visibility into that gap, but is not the pass/fail criterion.
    """
    evaluated = 0
    missing = 0
    docs: list[dict] = []
    for path in paths:
        if not path.exists():
            missing += 1
            continue
        envelope = run_extract(binary, path, cache_dir)
        if envelope is None:
            missing += 1
            continue
        evaluated += 1
        pages = implausible_pages(envelope) or []
        total_pages = page_count(envelope)
        docs.append(
            {
                "id": path.stem,
                "page_count": total_pages,
                "flagged_pages": pages,
                "any_flagged": len(pages) > 0,
                "fully_flagged": total_pages > 0 and len(pages) >= total_pages,
            }
        )
    any_flagged = sum(1 for doc in docs if doc["any_flagged"])
    fully_flagged = sum(1 for doc in docs if doc["fully_flagged"])
    recall = any_flagged / evaluated if evaluated else 0.0
    return {
        "evaluated": evaluated,
        "missing": missing,
        "docs": docs,
        "any_flagged": any_flagged,
        "fully_flagged": fully_flagged,
        "recall": recall,
    }


def print_cohort_row(name: str, report: dict, ceiling: float) -> bool:
    # ~keep: a cohort that evaluated nothing has proven nothing -- an empty or unfetched fixture
    # directory must read as a failure, not as a 0.00% false-positive rate.
    if report["evaluated"] == 0:
        status = "NO DOCUMENTS EVALUATED"
        ok = False
    else:
        ok = report["fp_rate"] <= ceiling
        status = "OK" if ok else "OVER CEILING"
    print(
        f"{name:<20} evaluated={report['evaluated']:>4} missing={report['missing']:>3} "
        f"flagged={len(report['flagged_ids']):>3} fp={report['fp_rate'] * 100:>5.2f}% "
        f"(ceiling {ceiling * 100:.1f}%)  {status}"
    )
    if report["flagged_ids"]:
        print(f"    flagged ids: {', '.join(report['flagged_ids'])}")
    return ok


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default="target/release/xberg", help="path to the release xberg CLI binary")
    parser.add_argument(
        "--fixtures-dir",
        default="tools/benchmark-harness/fixtures/pdf",
        help="directory of fixture JSON files carrying metadata.cohorts tags",
    )
    parser.add_argument(
        "--wrong-mapping-dir",
        default="test_documents/pdf/wrong_mapping",
        help="directory of wrong-mapping recall PDF fixtures",
    )
    args = parser.parse_args(argv)

    binary = Path(args.binary).resolve()
    if not binary.is_file():
        parser.error(
            f"CLI binary not found at {binary}; build it first with "
            "`cargo build -p xberg-cli --release --features pdf-ocr`"
        )

    fixtures_dir = Path(args.fixtures_dir).resolve()
    cohort_paths = (
        load_fixture_cohorts(fixtures_dir)
        if fixtures_dir.is_dir()
        else {cohort: [] for cohort in FALSE_POSITIVE_COHORTS}
    )

    wrong_mapping_dir = Path(args.wrong_mapping_dir).resolve()
    wrong_mapping_paths = sorted(wrong_mapping_dir.glob("*.pdf")) if wrong_mapping_dir.is_dir() else []

    cache_dir = Path(tempfile.mkdtemp(prefix="xberg-plausibility-calibrate-"))
    if not str(cache_dir) or not cache_dir.is_dir():
        parser.error("failed to create an isolated XBERG_CACHE_DIR; refusing to run uncached")

    overall_ok = True
    try:
        print(f"XBERG_CACHE_DIR={cache_dir}")
        print()
        for name in FALSE_POSITIVE_COHORTS:
            ceiling = FALSE_POSITIVE_CEILING_NATIVE_CLEAN if name == "native-clean" else FALSE_POSITIVE_CEILING_OTHER
            report = evaluate_false_positive_cohort(binary, cohort_paths.get(name, []), cache_dir)
            if not print_cohort_row(name, report, ceiling):
                overall_ok = False

        print()
        if wrong_mapping_paths:
            wm = evaluate_wrong_mapping_recall(binary, wrong_mapping_paths, cache_dir)
            ok = wm["recall"] >= WRONG_MAPPING_RECALL_TARGET
            status = "OK" if ok else "BELOW TARGET"
            overall_ok = overall_ok and ok
            print(
                f"wrong_mapping        evaluated={wm['evaluated']:>4} missing={wm['missing']:>3} "
                f"recall(any-page)={wm['any_flagged']}/{wm['evaluated']}={wm['recall'] * 100:.1f}% "
                f"(target {WRONG_MAPPING_RECALL_TARGET * 100:.0f}%)  {status}  "
                f"[fully-flagged={wm['fully_flagged']}/{wm['evaluated']}, reported for visibility only]"
            )
            for doc in wm["docs"]:
                print(
                    f"    {doc['id']}: {len(doc['flagged_pages'])}/{doc['page_count']} pages flagged "
                    f"{'(fully recalled)' if doc['fully_flagged'] else '(NOT fully recalled)'}"
                )
        else:
            print(f"wrong_mapping cohort: no fixtures found under {wrong_mapping_dir}")
            overall_ok = False

        return 0 if overall_ok else 1
    finally:
        shutil.rmtree(cache_dir, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
