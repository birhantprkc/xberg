"""Import OmniDocBench dataset into our benchmark fixture format.

Converts OmniDocBench's element-level JSON annotations into:
  - Per-document fixture JSON files (tools/benchmark-harness/fixtures/pdf/omnidoc_NNN.json)
  - Ground truth markdown files (test_documents/ground_truth/pdf/omnidoc_NNN.md)
  - Ground truth text files (test_documents/ground_truth/pdf/omnidoc_NNN.txt)

OmniDocBench groups pages by document. Each multi-page document produces one fixture.
Single-page documents produce one fixture per page.

Usage:
    python import_omnidocbench.py <omnidocbench_dir> <repo_root>

Where:
    omnidocbench_dir = tools/benchmark-harness/datasets/omnidocbench (contains OmniDocBench.json + ori_pdfs/)
    repo_root = repository root (contains tools/ and test_documents/)
"""

from __future__ import annotations

import html
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

CONTENT_CATEGORIES = {
    "title",
    "text_block",
    "table",
    "equation_isolated",
    "code_txt",
    "figure_caption",
    "table_caption",
    "equation_caption",
    "code_txt_caption",
    "reference",
}

SKIP_CATEGORIES = {
    "header",
    "footer",
    "page_number",
    "page_footnote",
    "abandon",
    "figure",
    "figure_footnote",
    "table_footnote",
}


def html_table_to_markdown(html_str: str) -> str:
    """Convert a simple HTML table to markdown table format."""
    if not html_str:
        return ""

    html_str = html.unescape(html_str)

    rows: list[list[str]] = []
    for row_match in re.finditer(r"<tr[^>]*>(.*?)</tr>", html_str, re.DOTALL):
        row_html = row_match.group(1)
        cells: list[str] = []
        for cell_match in re.finditer(r"<t[dh][^>]*>(.*?)</t[dh]>", row_html, re.DOTALL):
            cell_text = re.sub(r"<[^>]+>", "", cell_match.group(1)).strip()
            cells.append(cell_text)
        if cells:
            rows.append(cells)

    if not rows:
        return html_str

    max_cols = max(len(r) for r in rows)
    for row in rows:
        while len(row) < max_cols:
            row.append("")

    lines = []
    lines.append("| " + " | ".join(rows[0]) + " |")
    lines.append("|" + "|".join(["---"] * max_cols) + "|")
    for row in rows[1:]:
        lines.append("| " + " | ".join(row) + " |")

    return "\n".join(lines)


def annotation_to_markdown(ann: dict) -> str | None:
    """Convert a single OmniDocBench annotation to markdown text."""
    cat = ann.get("category_type", "")

    if cat in SKIP_CATEGORIES:
        return None

    if ann.get("ignore", False):
        return None

    text = ann.get("text", "").strip()

    if cat == "title":
        if text:
            return f"## {text}"
        return None

    if cat == "text_block":
        return text or None

    if cat == "table":
        html_str = ann.get("html", "")
        if html_str:
            return html_table_to_markdown(html_str)
        return text or None

    if cat == "equation_isolated":
        latex = ann.get("latex", "")
        if latex:
            return f"$$\n{latex}\n$$"
        return text or None

    if cat == "code_txt":
        if text:
            return f"```\n{text}\n```"
        return None

    if cat in ("figure_caption", "table_caption", "equation_caption", "code_txt_caption"):
        return text or None

    if cat == "reference":
        return text or None

    return text or None


def page_to_markdown(page: dict) -> str:
    """Convert a single OmniDocBench page to markdown."""
    annotations = page.get("layout_dets", [])

    sorted_anns = sorted(annotations, key=lambda a: a.get("order", 999))

    relations = page.get("extra", {}).get("relation", [])
    merge_targets: dict[int, int] = {}
    for rel in relations:
        if rel.get("relation") == "truncated":
            merge_targets[rel["target_anno_id"]] = rel["source_anno_id"]

    merged_text: dict[int, list[str]] = defaultdict(list)
    ann_by_id = {a.get("anno_id", i): a for i, a in enumerate(sorted_anns)}

    for ann in sorted_anns:
        anno_id = ann.get("anno_id", -1)
        if anno_id in merge_targets:
            source_id = merge_targets[anno_id]
            text = ann.get("text", "").strip()
            if text:
                merged_text[source_id].append(text)

    blocks: list[str] = []
    skip_ids = set(merge_targets.keys())

    for ann in sorted_anns:
        anno_id = ann.get("anno_id", -1)
        if anno_id in skip_ids:
            continue

        if anno_id in merged_text:
            original_text = ann.get("text", "").strip()
            continuation = " ".join(merged_text[anno_id])
            ann = dict(ann)
            ann["text"] = f"{original_text} {continuation}".strip()

        md = annotation_to_markdown(ann)
        if md:
            blocks.append(md)

    return "\n\n".join(blocks)


def strip_markdown_to_text(md: str) -> str:
    """Strip markdown syntax to produce plain text."""
    lines = []
    in_code = False
    in_formula = False

    for line in md.split("\n"):
        if line.startswith("```"):
            in_code = not in_code
            continue
        if line.startswith("$$"):
            in_formula = not in_formula
            continue
        if in_code or in_formula:
            lines.append(line)
            continue

        stripped = re.sub(r"^#{1,6}\s+", "", line)
        if stripped.startswith("|") and stripped.endswith("|"):
            if re.match(r"^\|[-|: ]+\|$", stripped):
                continue
            stripped = re.sub(r"\s*\|\s*", " ", stripped).strip()
        stripped = re.sub(r"\*{1,3}([^*]+)\*{1,3}", r"\1", stripped)

        if stripped:
            lines.append(stripped)

    return "\n".join(lines)


def group_pages_by_pdf(pages: list[dict]) -> dict[str, list[dict]]:
    """Group OmniDocBench pages by their source PDF."""
    groups: dict[str, list[dict]] = defaultdict(list)

    for page in pages:
        page_info = page.get("page_info", {})
        image_path = page_info.get("image_path", "")

        basename = os.path.splitext(os.path.basename(image_path))[0]

        pdf_name = re.sub(r"_p\d+$", "", basename)

        groups[pdf_name].append(page)

    for pdf_name in groups:
        groups[pdf_name].sort(key=lambda p: p.get("page_info", {}).get("page_no", 0))

    return groups


def find_pdf_by_stem_prefix(directory: Path, pdf_name: str) -> Path | None:
    """Return a PDF in `directory` whose stem is a prefix match for `pdf_name`, either way."""
    for pdf_file in directory.glob("*.pdf"):
        if pdf_name.startswith(pdf_file.stem) or pdf_file.stem.startswith(pdf_name):
            return pdf_file
    return None


def find_pdf_for_document(pdf_name: str, pages: list[dict], ori_pdfs_dir: Path) -> Path | None:
    """Find the original PDF file for a document group."""
    if not ori_pdfs_dir.exists():
        return None

    for ext in (".pdf", ".PDF"):
        candidate = ori_pdfs_dir / f"{pdf_name}{ext}"
        if candidate.exists():
            return candidate

    for pdf_file in ori_pdfs_dir.rglob("*.pdf"):
        if pdf_file.stem == pdf_name:
            return pdf_file

    if not pages:
        return None

    image_path = pages[0].get("page_info", {}).get("image_path", "")
    parts = image_path.split("/")
    if len(parts) < 2:
        return None

    subdir_path = ori_pdfs_dir / parts[0]
    if not subdir_path.exists():
        return None

    return find_pdf_by_stem_prefix(subdir_path, pdf_name)


# Outcomes of importing a single OmniDocBench document group; `main` uses these to keep its
# summary counters, so they must stay in sync with the branches in `import_single_document`. ~keep
IMPORT_STATUS_EXISTS = "exists"
IMPORT_STATUS_NO_PDF = "no_pdf"
IMPORT_STATUS_EMPTY = "empty"
IMPORT_STATUS_CREATED = "created"


def import_single_document(
    pdf_name: str,
    doc_pages: list[dict],
    ori_pdfs_dir: Path,
    fixtures_dir: Path,
    gt_dir: Path,
) -> str:
    """Import one OmniDocBench document group into a fixture and ground truth, if possible."""
    fixture_name = re.sub(r"[^a-zA-Z0-9_-]", "_", f"omnidoc_{pdf_name}")

    fixture_path = fixtures_dir / f"{fixture_name}.json"
    gt_md_path = gt_dir / f"{fixture_name}.md"
    gt_txt_path = gt_dir / f"{fixture_name}.txt"

    if fixture_path.exists():
        return IMPORT_STATUS_EXISTS

    pdf_path = find_pdf_for_document(pdf_name, doc_pages, ori_pdfs_dir)
    if pdf_path is None:
        return IMPORT_STATUS_NO_PDF

    page_markdowns = []
    for page in doc_pages:
        md = page_to_markdown(page)
        if md.strip():
            page_markdowns.append(md)

    if not page_markdowns:
        return IMPORT_STATUS_EMPTY

    full_markdown = "\n\n".join(page_markdowns)
    full_text = strip_markdown_to_text(full_markdown)

    gt_md_path.write_text(full_markdown)
    gt_txt_path.write_text(full_text)

    doc_rel = os.path.relpath(pdf_path, fixtures_dir)
    gt_md_rel = os.path.relpath(gt_md_path, fixtures_dir)
    gt_txt_rel = os.path.relpath(gt_txt_path, fixtures_dir)

    first_page = doc_pages[0].get("page_info", {})
    page_attr = first_page.get("page_attribute", {})

    fixture = {
        "document": doc_rel,
        "file_type": "pdf",
        "file_size": pdf_path.stat().st_size,
        "expected_frameworks": ["xberg"],
        "metadata": {
            "description": f"OmniDocBench: {page_attr.get('data_source', 'unknown')}",
            "source": "omnidocbench",
            "size_category": "small" if pdf_path.stat().st_size < 500_000 else "medium",
            "language": page_attr.get("language", "unknown"),
            "layout": page_attr.get("layout", "unknown"),
            "data_source": page_attr.get("data_source", "unknown"),
            "page_count": len(doc_pages),
        },
        "ground_truth": {
            "text_file": gt_txt_rel,
            "markdown_file": gt_md_rel,
            "source": "omnidocbench",
        },
    }

    fixture_path.write_text(json.dumps(fixture, indent=2) + "\n")
    return IMPORT_STATUS_CREATED


def main() -> None:
    if len(sys.argv) < 3:
        print(
            "Usage: import_omnidocbench.py <omnidocbench_dir> <repo_root>",
            file=sys.stderr,
        )
        sys.exit(1)

    omnidoc_dir = Path(sys.argv[1]).resolve()
    repo_root = Path(sys.argv[2]).resolve()

    json_path = omnidoc_dir / "OmniDocBench.json"
    ori_pdfs_dir = omnidoc_dir / "ori_pdfs"

    if not json_path.exists():
        print(f"ERROR: {json_path} not found. Run download_omnidocbench.sh first.", file=sys.stderr)
        sys.exit(1)

    fixtures_dir = repo_root / "tools" / "benchmark-harness" / "fixtures" / "pdf"
    gt_dir = repo_root / "test_documents" / "ground_truth" / "pdf"
    fixtures_dir.mkdir(parents=True, exist_ok=True)
    gt_dir.mkdir(parents=True, exist_ok=True)

    print(f"Loading {json_path}...", file=sys.stderr)
    with open(json_path) as f:
        pages = json.load(f)
    print(f"Loaded {len(pages)} pages", file=sys.stderr)

    doc_groups = group_pages_by_pdf(pages)
    print(f"Found {len(doc_groups)} documents", file=sys.stderr)

    created = 0
    skipped_no_pdf = 0
    skipped_exists = 0
    skipped_empty = 0

    for pdf_name, doc_pages in sorted(doc_groups.items()):
        status = import_single_document(pdf_name, doc_pages, ori_pdfs_dir, fixtures_dir, gt_dir)

        if status == IMPORT_STATUS_EXISTS:
            skipped_exists += 1
            continue
        if status == IMPORT_STATUS_NO_PDF:
            skipped_no_pdf += 1
            continue
        if status == IMPORT_STATUS_EMPTY:
            skipped_empty += 1
            continue

        created += 1
        if created % 50 == 0:
            print(f"  {created} fixtures created...", file=sys.stderr)

    print("\nDone:", file=sys.stderr)
    print(f"  Created: {created}", file=sys.stderr)
    print(f"  Skipped (already exists): {skipped_exists}", file=sys.stderr)
    print(f"  Skipped (no PDF found): {skipped_no_pdf}", file=sys.stderr)
    print(f"  Skipped (empty content): {skipped_empty}", file=sys.stderr)
    print(f"  Fixtures: {fixtures_dir}", file=sys.stderr)
    print(f"  Ground truth: {gt_dir}", file=sys.stderr)


if __name__ == "__main__":
    main()
