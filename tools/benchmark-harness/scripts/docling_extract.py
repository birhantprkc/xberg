"""Docling extraction wrapper for benchmark harness.

Supports three modes:
- sync: DocumentConverter.convert() - synchronous single-file extraction
- batch: docling-jobkit DoclingConverterManager - in-process batch extraction
- server: persistent mode reading paths from stdin
"""

from __future__ import annotations

import json
import multiprocessing as _mp
import os
import platform
import resource
import sys
import time
from typing import Any

from docling.document_converter import DocumentConverter


def _get_peak_memory_bytes() -> int:
    """Get peak memory usage in bytes using resource module."""
    usage = resource.getrusage(resource.RUSAGE_SELF)
    if platform.system() == "Linux":
        return usage.ru_maxrss * 1024
    return usage.ru_maxrss


def _tesseract_to_easyocr_langs(ocr_language: str | None) -> list[str]:
    """Map a canonical Tesseract language string (``eng+kor``, ``jpn_vert``) to the
    EasyOCR language list Docling's default OCR engine expects. EasyOCR uses ISO-639-1
    codes and combines one non-Latin script with English at a time, which matches how
    the fixtures pin a single script. An unset value returns an empty list so EasyOCR's
    default is left untouched.
    """
    if not ocr_language:
        return []
    mapping = {"eng": "en", "kor": "ko", "jpn": "ja", "jpn_vert": "ja", "chi_sim": "ch_sim", "chi_tra": "ch_tra"}
    langs: list[str] = []
    for raw in ocr_language.split("+"):
        mapped = mapping.get(raw.strip().lower())
        if mapped and mapped not in langs:
            langs.append(mapped)
    return langs


def create_converter(ocr_enabled: bool, ocr_language: str | None = None) -> DocumentConverter:
    """Create a DocumentConverter with OCR explicitly toggled.

    In docling 2.x, ``DocumentConverter`` does not accept a ``pipeline_options``
    kwarg; per-format pipeline options must be passed via ``format_options``.
    Passing ``pipeline_options`` is silently wrong: the converter falls back to
    its default PDF pipeline, which pulls an incompatible PP-OCRv6 torch model
    and fails every PDF/image extraction. We build explicit PDF and image
    format options with ``do_ocr`` set from ``ocr_enabled``.
    """
    try:
        from docling.datamodel.base_models import InputFormat
        from docling.datamodel.pipeline_options import PdfPipelineOptions
        from docling.document_converter import ImageFormatOption, PdfFormatOption

        pdf_options = PdfPipelineOptions()
        pdf_options.do_ocr = ocr_enabled
        easyocr_langs = _tesseract_to_easyocr_langs(ocr_language)
        if ocr_enabled and easyocr_langs:
            # Docling's default OCR engine is EasyOCR; override its language list so the
            # fixture's script (Korean/Japanese) is recognized instead of the eng default.
            pdf_options.ocr_options.lang = easyocr_langs
        return DocumentConverter(
            format_options={
                InputFormat.PDF: PdfFormatOption(pipeline_options=pdf_options),
                InputFormat.IMAGE: ImageFormatOption(pipeline_options=pdf_options),
            }
        )
    except (ImportError, TypeError) as exc:
        state = "enabled" if ocr_enabled else "disabled"
        raise RuntimeError(f"failed to configure Docling with OCR explicitly {state}") from exc


def _render(document: Any, output_format: str) -> str:
    if output_format == "plaintext":
        return document.export_to_text(traverse_pictures=True)
    return document.export_to_markdown(traverse_pictures=True)


def extract_sync(file_path: str, converter: DocumentConverter, output_format: str = "markdown") -> dict[str, Any]:
    """Extract using synchronous single-file API."""
    start = time.perf_counter()
    result = converter.convert(file_path)
    content = _render(result.document, output_format)
    duration_ms = (time.perf_counter() - start) * 1000.0

    return {
        "content": content,
        "metadata": {"framework": "docling", "output_format": output_format},
        "_extraction_time_ms": duration_ms,
        "_peak_memory_bytes": _get_peak_memory_bytes(),
    }


def _build_batch_convert_options(ocr_enabled: bool, ocr_language: str | None) -> Any:
    """Build jobkit's ConvertDocumentsOptions for a batch run, mirroring the single-file OCR flag."""
    from docling.datamodel.base_models import OutputFormat
    from docling_jobkit.datamodel.convert import ConvertDocumentsOptions

    # ``to_formats`` drives jobkit's own writer, which we bypass (we render from the
    # DoclingDocument ourselves), so it only needs to be a valid default. ``do_ocr`` mirrors the
    # harness OCR flag; ``abort_on_error`` stays False so one bad document does not sink the batch.
    option_kwargs: dict[str, Any] = {
        "to_formats": [OutputFormat.MARKDOWN],
        "do_ocr": ocr_enabled,
        "abort_on_error": False,
    }
    easyocr_langs = _tesseract_to_easyocr_langs(ocr_language) if ocr_enabled else []
    if easyocr_langs:
        # jobkit's ConvertDocumentsOptions forwards ocr_lang to the same EasyOCR engine
        # as the single-file path; only set it when the fixture pins a language.
        option_kwargs["ocr_lang"] = easyocr_langs
    return ConvertDocumentsOptions(**option_kwargs)


def _render_batch_result(result: Any, output_format: str) -> dict[str, Any]:
    """Render one jobkit conversion result into the harness's per-file output shape."""
    from docling.datamodel.base_models import ConversionStatus

    # PARTIAL_SUCCESS means docling converted the document but hit recoverable problems on
    # some pages; the DoclingDocument still carries real content. Rendering only SUCCESS
    # scored those documents as empty, which understated docling in batch mode while the
    # single-file path (``extract_sync``) renders them unconditionally. Render whenever a
    # document is present and carry the degraded status through as metadata. ~keep
    if result.document is not None and result.status in (
        ConversionStatus.SUCCESS,
        ConversionStatus.PARTIAL_SUCCESS,
    ):
        content = _render(result.document, output_format)
        metadata: dict[str, Any] = {"framework": "docling", "output_format": output_format}
        if result.status != ConversionStatus.SUCCESS:
            metadata["status"] = result.status.name
            if result.errors:
                metadata["partial_errors"] = str(result.errors)
        return {
            "content": content,
            "metadata": metadata,
            "_peak_memory_bytes": _get_peak_memory_bytes(),
        }

    error = str(result.errors) if result.errors else "Unknown error"
    return {
        "content": "",
        "error": error,
        "metadata": {
            "framework": "docling",
            "error": error,
            "status": result.status.name,
        },
        "_peak_memory_bytes": _get_peak_memory_bytes(),
    }


def extract_batch(
    file_paths: list[str], ocr_enabled: bool, output_format: str = "markdown", ocr_language: str | None = None
) -> dict[str, Any]:
    """Extract multiple files using docling-jobkit's in-process converter manager.

    docling-jobkit's ``DoclingConverterManager`` is the batch engine (it wraps docling's
    ``convert_all`` with jobkit's option handling and the docling-slim pipeline). It is driven
    in-process and single-process here: the harness times this whole subprocess as one cold batch,
    so jobkit's own multiprocessing is intentionally not used. Unlike a fixed-partition batch it
    accepts any number of eligible documents. Rendering is done locally (``_render``) so batch
    output matches the single-file path exactly. ``convert_all`` preserves input order and count,
    so ``results`` stays 1:1 with ``file_paths``.
    """
    from docling_jobkit.convert.manager import (
        DoclingConverterManager,
        DoclingConverterManagerConfig,
    )

    manager = DoclingConverterManager(config=DoclingConverterManagerConfig(options_cache_size=1))
    options = _build_batch_convert_options(ocr_enabled, ocr_language)

    start = time.perf_counter()
    outputs: list[dict[str, Any]] = []
    # Consume the conversion stream lazily in a single pass: render each document as it arrives
    # (``convert_all`` under the hood preserves input order and count, so ``outputs`` stays 1:1
    # with ``file_paths``). ~keep
    for result in manager.convert_documents(sources=list(file_paths), options=options):
        outputs.append(_render_batch_result(result, output_format))

    total_duration_ms = (time.perf_counter() - start) * 1000.0
    return {
        "results": outputs,
        "total_ms": total_duration_ms,
        "per_file_ms": [None] * len(outputs),
        "metadata": {
            "framework": "docling",
            "batch_api": "docling_jobkit.DoclingConverterManager.convert_documents",
            "reported_total_timing_scope": "jobkit_convert_documents_iteration_and_render",
            "benchmark_timing_scope": "cold_end_to_end_subprocess",
            "per_item_timing": "unavailable",
        },
    }


def _worker(fn, args, conn):
    """Run extraction in a forked child process.

    Closes inherited stdin/stdout so the child cannot corrupt the
    parent's line-based JSON protocol.
    """
    try:
        sys.stdin.close()
        sys.stdout = open(os.devnull, "w")
    except Exception:
        pass
    try:
        result = fn(*args)
        conn.send(result)
    except Exception as e:
        conn.send({"error": str(e), "_extraction_time_ms": 0})
    finally:
        conn.close()


def _run_with_timeout(fn, args, timeout):
    """Execute fn(*args) in a forked child with a timeout.

    On timeout the child is killed but the parent stays alive —
    no expensive process restart is needed.
    """
    try:
        ctx = _mp.get_context("fork")
        parent_conn, child_conn = ctx.Pipe(duplex=False)
        p = ctx.Process(target=_worker, args=(fn, args, child_conn))
        p.start()
        child_conn.close()

        if parent_conn.poll(timeout=timeout):
            try:
                result = parent_conn.recv()
            except Exception:
                result = {"error": "worker process crashed", "_extraction_time_ms": 0}
        else:
            p.kill()
            result = {
                "error": f"extraction timed out after {timeout}s",
                "_extraction_time_ms": timeout * 1000.0,
            }

        p.join(timeout=5)
        if p.is_alive():
            p.kill()
            p.join()
        parent_conn.close()
        return result
    except Exception:
        try:
            return fn(*args)
        except Exception as e:
            return {"error": str(e), "_extraction_time_ms": 0}


def _parse_path(line: str) -> str:
    """Parse a request line: JSON object with path field, or plain file path."""
    stripped = line.strip()
    if stripped.startswith("{"):
        try:
            return json.loads(stripped).get("path", "")
        except (json.JSONDecodeError, ValueError):
            pass
    return stripped


def run_server(converter: DocumentConverter, output_format: str, timeout=None) -> None:
    """Persistent server mode: read paths from stdin, write JSON to stdout."""
    print("READY", flush=True)
    for line in sys.stdin:
        file_path = _parse_path(line)
        if not file_path:
            continue
        if timeout is not None:
            result = _run_with_timeout(extract_sync, (file_path, converter, output_format), timeout)
        else:
            try:
                result = extract_sync(file_path, converter, output_format)
            except Exception as e:
                result = {"error": str(e), "_extraction_time_ms": 0}
        print(json.dumps(result), flush=True)


def main() -> None:
    ocr_enabled = False
    timeout = None
    output_format = "markdown"
    ocr_language = None
    args = []
    for arg in sys.argv[1:]:
        if arg == "--ocr":
            ocr_enabled = True
        elif arg == "--no-ocr":
            ocr_enabled = False
        elif arg.startswith("--timeout="):
            timeout = int(arg.split("=", 1)[1])
        elif arg.startswith("--format="):
            output_format = arg.split("=", 1)[1]
        elif arg.startswith("--ocr-lang="):
            ocr_language = arg.split("=", 1)[1]
        elif arg == "--format":
            args.append(arg)
        else:
            args.append(arg)

    cleaned: list[str] = []
    i = 0
    while i < len(args):
        if args[i] == "--format" and i + 1 < len(args):
            output_format = args[i + 1]
            i += 2
            continue
        cleaned.append(args[i])
        i += 1
    args = cleaned

    if output_format not in ("markdown", "plaintext"):
        print(f"Error: --format must be 'markdown' or 'plaintext'; got '{output_format}'", file=sys.stderr)
        sys.exit(64)

    if len(args) < 1:
        print(
            "Usage: docling_extract.py [--ocr|--no-ocr] [--timeout=SECS] [--format markdown|plaintext] <mode> <file_path> [additional_files...]",
            file=sys.stderr,
        )
        print("Modes: sync, batch, server", file=sys.stderr)
        sys.exit(1)

    mode = args[0]
    file_paths = args[1:]

    try:
        if mode == "server":
            converter = create_converter(ocr_enabled, ocr_language)
            run_server(converter, output_format, timeout=timeout)

        elif mode == "sync":
            if len(file_paths) != 1:
                print("Error: sync mode requires exactly one file", file=sys.stderr)
                sys.exit(1)
            converter = create_converter(ocr_enabled, ocr_language)
            payload = extract_sync(file_paths[0], converter, output_format)
            print(json.dumps(payload), end="")

        elif mode == "batch":
            if len(file_paths) < 1:
                print("Error: batch mode requires at least one file", file=sys.stderr)
                sys.exit(1)

            # Batch uses docling-jobkit's converter manager, not the single-file DocumentConverter.
            payload = extract_batch(file_paths, ocr_enabled, output_format, ocr_language)
            print(json.dumps(payload), end="")

        else:
            print(f"Error: Unknown mode '{mode}'. Use sync, batch, or server", file=sys.stderr)
            sys.exit(1)

    except Exception as e:
        print(f"Error extracting with Docling: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
