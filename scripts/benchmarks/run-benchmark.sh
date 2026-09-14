#!/usr/bin/env bash
set -euo pipefail

FRAMEWORK="${FRAMEWORK:-}"
MODE="${MODE:-}"
ITERATIONS="${ITERATIONS:-3}"
TIMEOUT="${TIMEOUT:-900}"
FIXTURES_DIR="${FIXTURES_DIR:-tools/benchmark-harness/fixtures}"
HARNESS_PATH="${HARNESS_PATH:-./target/release/benchmark-harness}"
MEASURE_QUALITY="${MEASURE_QUALITY:-false}"
OCR_ENABLED="${OCR_ENABLED:-false}"
OUTPUT_FORMAT="${OUTPUT_FORMAT:-markdown}"
COHORT="${COHORT:-}"
BATCH_SIZE="${BATCH_SIZE:-}"
SHARD="${SHARD:-}"
XBERG_MAX_THREADS="${XBERG_MAX_THREADS:-4}"
# PDF backends to pass through to `run --pdf-backends` (comma-separated: native, pdfium).
# Unset by every caller except the pdfium leg -- omitting it keeps today's native-only default. ~keep
PDF_BACKENDS="${PDF_BACKENDS:-}"

if [ -z "$FRAMEWORK" ] || [ -z "$MODE" ]; then
  echo "::error::FRAMEWORK and MODE environment variables are required" >&2
  exit 1
fi

if [ -n "$COHORT" ]; then
  if [ ! -f "$COHORT" ]; then
    echo "::error::Benchmark cohort does not exist: $COHORT" >&2
    exit 1
  fi
  if ! jq -e '
    type == "object" and
    .schema_version == 1 and
    (.name | type == "string" and length > 0) and
    (.batch_size | type == "number" and . > 0 and floor == .) and
    (.fixtures | type == "array" and length > 0) and
    ((.fixtures | length) % .batch_size == 0)
  ' "$COHORT" >/dev/null 2>&1; then
    echo "::error::Benchmark cohort has an invalid manifest shape: $COHORT" >&2
    exit 1
  fi
  # The manifest is authoritative for OCR and batch size, so the workflow no longer keys these
  # off the cohort name. A legacy manifest without `ocr_enabled` defaults to non-OCR.
  OCR_ENABLED="$(jq -r '.ocr_enabled // false' "$COHORT")"
  if [ "$MODE" = "batch" ] && [ -z "$BATCH_SIZE" ]; then
    BATCH_SIZE="$(jq -r '.batch_size' "$COHORT")"
  fi
fi

if [ -n "$COHORT" ] && [ -n "$SHARD" ]; then
  echo "::error::COHORT and SHARD cannot be used together" >&2
  exit 1
fi

if [ -n "$BATCH_SIZE" ]; then
  if [[ ! "$BATCH_SIZE" =~ ^[1-9][0-9]*$ ]]; then
    echo "::error::BATCH_SIZE must be a positive integer: $BATCH_SIZE" >&2
    exit 1
  fi
  if [ "$MODE" != "batch" ]; then
    echo "::error::BATCH_SIZE is only valid when MODE=batch" >&2
    exit 1
  fi
  if [ -n "$COHORT" ]; then
    COHORT_BATCH_SIZE="$(jq -r '.batch_size' "$COHORT")"
    if [ "$BATCH_SIZE" != "$COHORT_BATCH_SIZE" ]; then
      echo "::error::BATCH_SIZE $BATCH_SIZE does not match cohort batch_size $COHORT_BATCH_SIZE: $COHORT" >&2
      exit 1
    fi
  fi
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "${REPO_ROOT}/scripts/lib/common.sh"
source "${REPO_ROOT}/scripts/lib/library-paths.sh"

validate_repo_root "$REPO_ROOT" || exit 1

setup_go_paths "$REPO_ROOT"
setup_onnx_paths

OUTPUT_DIR="benchmark-results/${FRAMEWORK}-${OUTPUT_FORMAT}-${MODE}"
rm -rf "${OUTPUT_DIR}"

MAX_CONCURRENT=$([[ "$MODE" == "single-file" ]] && echo 1 || echo 4)

EXTRA_ARGS=()
if [ "$MEASURE_QUALITY" = "true" ]; then
  EXTRA_ARGS+=("--measure-quality")
fi
if [ "$OCR_ENABLED" = "true" ]; then
  EXTRA_ARGS+=("--ocr")
fi
# Xberg is the subject under test and stays strict (the CLI default of 1.0). ~keep
# Competitor runs may retain useful measurements when isolated supported-document
# extractions fail, but still reject a framework that fails most attempted documents.
case "$FRAMEWORK" in
xberg | xberg-*) ;;
*) EXTRA_ARGS+=("--min-success-rate" "${MIN_SUCCESS_RATE:-0.5}") ;;
esac
if [ -n "$SHARD" ]; then
  EXTRA_ARGS+=("--shard" "${SHARD}")
fi
if [ -n "$COHORT" ]; then
  EXTRA_ARGS+=("--cohort" "${COHORT}")
fi
if [ -n "$BATCH_SIZE" ]; then
  EXTRA_ARGS+=("--batch-size" "${BATCH_SIZE}")
fi
if [ -n "$PDF_BACKENDS" ]; then
  EXTRA_ARGS+=("--pdf-backends" "${PDF_BACKENDS}")
fi

BENCHMARK_DEBUG=1 "${HARNESS_PATH}" \
  run \
  --fixtures "${FIXTURES_DIR}" \
  --frameworks "${FRAMEWORK}" \
  --output "${OUTPUT_DIR}" \
  --iterations "${ITERATIONS}" \
  --timeout "${TIMEOUT}" \
  --mode "${MODE}" \
  --max-concurrent "${MAX_CONCURRENT}" \
  --xberg-max-threads "${XBERG_MAX_THREADS}" \
  --output-format "${OUTPUT_FORMAT}" \
  "${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}"

# Guard against a green run that silently measured nothing (this repo has a history of
# feature-gated / misrouted legs passing vacuously). A pdfium leg that registers zero rows
# would otherwise report success while the pdfium backend was never actually exercised.
if printf '%s' "$PDF_BACKENDS" | tr ',' '\n' | grep -qx "pdfium"; then
  RESULTS_FILE="${OUTPUT_DIR}/results.json"
  if [ ! -f "$RESULTS_FILE" ]; then
    echo "::error::pdfium leg did not produce ${RESULTS_FILE}" >&2
    exit 1
  fi
  # Count PDFIUM rows specifically, not total rows: a native-only result set is also non-empty,
  # so a bare `length` check would pass while the pdfium backend went unexercised -- the exact
  # vacuous green this guard exists to prevent.
  # `contains`, not `endswith`: batch mode appends a `-batch` suffix AFTER the backend
  # suffix (`xberg-markdown-baseline-pdfium-batch`, see main.rs's framework_name), so an
  # endswith("-pdfium") check would match nothing on half the matrix and fail spuriously.
  ROW_COUNT="$(jq '[.[] | select(.framework | contains("-pdfium"))] | length' "$RESULTS_FILE")"
  if [ "$ROW_COUNT" -eq 0 ]; then
    echo "::error::pdfium leg produced zero -pdfium rows in ${RESULTS_FILE} -- the run reported success but never exercised the pdfium backend" >&2
    exit 1
  fi
  echo "pdfium leg produced ${ROW_COUNT} -pdfium row(s) in ${RESULTS_FILE}"
fi
