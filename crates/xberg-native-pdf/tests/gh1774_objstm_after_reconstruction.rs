//! GH#1774: a truncated final `/XRef` stream forces full-file reconstruction,
//! which finds objects only via a literal "N G obj" header scan and is
//! therefore blind to any object that lives solely inside a compressed
//! `/ObjStm` container. Two real-world PDFs lost their font resources this
//! way and extracted 86/88 and 22/24 pages as empty, with zero errors.
//!
//! This builds a minimal synthetic PDF reproducing the same shape (truncated
//! `/XRef` stream + an object reachable only inside an `/ObjStm`) so the
//! regression test is self-contained and fast, plus two negative controls
//! proving the new `XrefRecovery` warning does not fire on a healthy
//! document or on a legitimately-deleted (`§7.3.10`) reference.

use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;
use xberg_native_pdf::PdfDocument;
use xberg_native_pdf::extractors::warnings::WarningCategory;
use xberg_native_pdf::object::ObjectRef;

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// Big-endian bytes of `value`, exactly `width` bytes wide.
fn be_width(value: u64, width: usize) -> Vec<u8> {
    value.to_be_bytes()[8 - width..].to_vec()
}

/// A single `/W [1 4 2]` xref-stream entry: 1-byte type, 4-byte field2, 2-byte field3.
fn xref_entry(entry_type: u8, field2: u64, field3: u64) -> Vec<u8> {
    let mut e = vec![entry_type];
    e.extend(be_width(field2, 4));
    e.extend(be_width(field3, 2));
    e
}

/// Builds a minimal PDF with:
///   1 = Catalog, 2 = Pages, 3 = Page (/Resources /Font /F1 -> `font_ref`),
///   5 = Contents stream, 6 = ObjStm packing object 4 (a real Font dict).
///
/// The page's `/F1` entry points at `font_ref`: pass `4` to reference the
/// ObjStm-packed (recoverable) font, or a number that exists nowhere in the
/// file (e.g. `9`) to reproduce a genuinely unrecoverable reference.
///
/// The trailing `/XRef` stream (object 7) is real, valid, zlib-compressed
/// data that is then truncated by `truncate_by` bytes — reproducing the
/// "incomplete deflate stream" shape of the real bug — so `PdfDocument::open`
/// is forced through full-file reconstruction exactly as it is for the two
/// real report documents.
fn build_pdf(font_ref: u32, truncate_by: usize) -> Vec<u8> {
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n");

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off3 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 {font_ref} 0 R >> >> /Contents 5 0 R >>\nendobj\n"
        )
        .as_bytes(),
    );

    let content = b"BT /F1 12 Tf 72 700 Td (Hi) Tj ET";
    let off5 = pdf.len();
    pdf.extend_from_slice(format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // Object stream packing object 4 — a real Font dict, reachable ONLY here
    // (no standalone "4 0 obj" header anywhere in the file). ~keep
    let font_dict = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";
    let header = b"4 0\n"; // pairs of (obj_num, offset-from-/First) ~keep
    let mut objstm_body = Vec::new();
    objstm_body.extend_from_slice(header);
    objstm_body.extend_from_slice(font_dict);
    let objstm_compressed = zlib_compress(&objstm_body);

    let off6 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /ObjStm /N 1 /First {} /Filter /FlateDecode /Length {} >>\nstream\n",
            header.len(),
            objstm_compressed.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&objstm_compressed);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // A real, valid xref-stream payload, /W [1 4 2], covering objects 0-7 (7
    // is this stream object itself) — then truncated to reproduce the bug. ~keep
    let mut xref_data = Vec::new();
    xref_data.extend(xref_entry(0, 0, 65535)); // 0: free ~keep
    xref_data.extend(xref_entry(1, off1 as u64, 0));
    xref_data.extend(xref_entry(1, off2 as u64, 0));
    xref_data.extend(xref_entry(1, off3 as u64, 0));
    xref_data.extend(xref_entry(0, 0, 0)); // 4: not directly resolvable via this table either ~keep
    xref_data.extend(xref_entry(1, off5 as u64, 0));
    xref_data.extend(xref_entry(1, off6 as u64, 0));
    let off7 = pdf.len();
    xref_data.extend(xref_entry(1, off7 as u64, 0));
    let xref_compressed_full = zlib_compress(&xref_data);
    let cut = xref_compressed_full.len().saturating_sub(truncate_by);
    let xref_compressed = &xref_compressed_full[..cut];

    pdf.extend_from_slice(
        format!(
            "7 0 obj\n<< /Type /XRef /Size 8 /W [1 4 2] /Index [0 8] /Root 1 0 R \
             /Filter /FlateDecode /Length {} >>\nstream\n",
            xref_compressed.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(xref_compressed);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    pdf.extend_from_slice(format!("startxref\n{off7}\n%%EOF").as_bytes());
    pdf
}

/// A healthy, complete, TRADITIONAL xref+trailer PDF — no reconstruction
/// needed. Object 3's `/Contents` reference is deliberately dangling (points
/// at object 99, which does not exist anywhere), so `load_object` resolves it
/// to `Null` via the exact same `None` arm as the reconstructed-document
/// case, but WITHOUT `xref_reconstructed` being true.
fn build_healthy_pdf_with_dangling_ref() -> Vec<u8> {
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 99 0 R >>\nendobj\n",
    );

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off3).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF").as_bytes());
    pdf
}

/// A healthy, complete, TRADITIONAL xref+trailer PDF where object 3 is
/// explicitly marked FREE ("f") — a legitimate deletion per §7.3.10 — and is
/// referenced from the page tree. Resolves to `Null` via the *other* branch
/// of `load_object` (`!entry.in_use`), never the reconstruction-only branch.
fn build_healthy_pdf_with_legitimately_freed_object() -> Vec<u8> {
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    // References object 3, which the xref below marks free (deleted). ~keep
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off1).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \n", off2).as_bytes());
    pdf.extend_from_slice(b"0000000000 00001 f \n"); // object 3: free ~keep
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF").as_bytes());
    pdf
}

/// The truncated `/XRef` stream must actually force reconstruction, and the
/// ObjStm-packed font must actually be absent from the reconstructed table —
/// otherwise this whole fixture is measuring nothing. Prove both.
#[test]
fn fixture_sanity_truncated_xref_forces_reconstruction_and_hides_objstm_object() {
    let pdf = build_pdf(4, 12);

    // The regular (non-reconstruction) xref/trailer parse must NOT produce a
    // usable table: either an outright error, or — because the truncated
    // stream's own bytes don't start with a literal "xref" keyword — the
    // traditional-table fallback inside `parse_xref_iterative` finding
    // nothing and returning an empty table. `open_from_bytes_inner` treats
    // both the same way (`xref.is_empty()` also triggers reconstruction), so
    // either outcome is a faithful trigger for the real code path; only a
    // populated table would mean this fixture isn't testing anything. ~keep
    let mut cursor = std::io::Cursor::new(pdf.clone());
    let offset = xberg_native_pdf::xref::find_xref_offset(&mut cursor).expect("startxref must be found");
    let regular_parse = xberg_native_pdf::xref::parse_xref(&mut cursor, offset);
    let forces_reconstruction = match &regular_parse {
        Err(_) => true,
        Ok(table) => table.is_empty(),
    };
    assert!(
        forces_reconstruction,
        "fixture must force a genuinely truncated /XRef stream to yield no usable table, got: {regular_parse:?}"
    );

    // Reconstruction succeeds but must NOT contain object 4. ~keep
    let mut cursor2 = std::io::Cursor::new(pdf);
    let (xref, _trailer, _synthetic) =
        xberg_native_pdf::xref_reconstruction::reconstruct_xref(&mut cursor2).expect("reconstruction must succeed");
    assert!(
        !xref.contains(4),
        "fixture must genuinely hide object 4 from the header-scan reconstruction"
    );
}

/// RED before the fix: font object 4 lives only inside the ObjStm, the
/// reconstructed xref never sees it, and `load_object` returns `Null`.
/// GREEN after the fix: the proactive ObjStm sweep in `open.rs` recovers it,
/// and `load_object` returns the real Font dictionary.
#[test]
fn objstm_packed_object_resolves_after_xref_reconstruction() {
    let pdf = build_pdf(4, 12);
    let doc = PdfDocument::from_bytes(pdf).expect("document must open via reconstruction");

    let font = doc
        .load_object(ObjectRef::new(4, 0))
        .expect("load_object must not error for a recoverable ObjStm-packed object");

    let dict = font
        .as_dict()
        .unwrap_or_else(|| panic!("expected a Dictionary, got {font:?}"));
    assert_eq!(
        dict.get("BaseFont").and_then(|o| o.as_name()),
        Some("Helvetica"),
        "the ObjStm-packed Font dictionary's own field must round-trip, not just a Null placeholder"
    );
}

/// A reference that is genuinely absent (no header, no ObjStm anywhere) even
/// after reconstruction AND the ObjStm sweep must still resolve to `Null`
/// (§7.3.10) — recovery has limits — but MUST now raise the `XrefRecovery`
/// structured warning, since silence here is exactly what let GH#1774 hide.
#[test]
fn genuinely_unrecoverable_reference_after_reconstruction_raises_warning() {
    let pdf = build_pdf(9, 12); // object 9 exists nowhere: not a header, not in any ObjStm ~keep
    let doc = PdfDocument::from_bytes(pdf).expect("document must open via reconstruction");

    let resolved = doc
        .load_object(ObjectRef::new(9, 0))
        .expect("missing refs resolve to Null, not Err");
    assert!(matches!(resolved, xberg_native_pdf::object::Object::Null));

    let warnings = doc.structured_warnings();
    let xref_recovery_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.category == WarningCategory::XrefRecovery)
        .collect();
    assert_eq!(
        xref_recovery_warnings.len(),
        1,
        "expected exactly one XrefRecovery warning for the one unrecoverable reference, got: {warnings:?}"
    );
    assert!(
        xref_recovery_warnings[0].message.contains('9'),
        "warning should name the missing object"
    );
    assert_eq!(xref_recovery_warnings[0].spec_section, Some("7.3.10"));
}

/// NEGATIVE CONTROL 1: a healthy document whose xref parses normally (no
/// reconstruction at all) with a dangling reference must resolve to `Null`
/// via the exact same `load_object` code path — but must NOT raise the new
/// warning. Proves the warning is gated on reconstruction, not on "any Null".
#[test]
fn healthy_document_dangling_reference_does_not_warn() {
    let pdf = build_healthy_pdf_with_dangling_ref();

    // Confirm this fixture does NOT need reconstruction, or the control is meaningless. ~keep
    let mut cursor = std::io::Cursor::new(pdf.clone());
    let offset = xberg_native_pdf::xref::find_xref_offset(&mut cursor).expect("startxref must be found");
    let regular_parse = xberg_native_pdf::xref::parse_xref(&mut cursor, offset);
    assert!(
        regular_parse.is_ok(),
        "control fixture's xref must parse normally: {regular_parse:?}"
    );
    assert!(
        !regular_parse.unwrap().is_empty(),
        "control fixture's parsed xref must be non-empty"
    );

    let doc = PdfDocument::from_bytes(pdf).expect("healthy document must open");
    let resolved = doc
        .load_object(ObjectRef::new(99, 0))
        .expect("dangling ref resolves to Null, not Err");
    assert!(matches!(resolved, xberg_native_pdf::object::Object::Null));

    let warnings = doc.structured_warnings();
    assert!(
        warnings.iter().all(|w| w.category != WarningCategory::XrefRecovery),
        "a healthy, non-reconstructed document must never raise XrefRecovery, got: {warnings:?}"
    );
}

/// NEGATIVE CONTROL 2: a legitimately-freed object (§7.3.10 deletion) in a
/// normally-parsed xref resolves to `Null` via the `!entry.in_use` branch —
/// a different branch entirely from the reconstruction-only one — and must
/// NOT raise the new warning either.
#[test]
fn legitimately_freed_object_does_not_warn() {
    let pdf = build_healthy_pdf_with_legitimately_freed_object();

    let mut cursor = std::io::Cursor::new(pdf.clone());
    let offset = xberg_native_pdf::xref::find_xref_offset(&mut cursor).expect("startxref must be found");
    let regular_parse = xberg_native_pdf::xref::parse_xref(&mut cursor, offset).expect("control fixture must parse");
    assert!(
        !regular_parse.get(3).expect("object 3 entry must exist").in_use,
        "control fixture must actually mark object 3 free"
    );

    let doc = PdfDocument::from_bytes(pdf).expect("healthy document must open");
    let resolved = doc
        .load_object(ObjectRef::new(3, 0))
        .expect("freed ref resolves to Null, not Err");
    assert!(matches!(resolved, xberg_native_pdf::object::Object::Null));

    let warnings = doc.structured_warnings();
    assert!(
        warnings.iter().all(|w| w.category != WarningCategory::XrefRecovery),
        "a legitimately-freed §7.3.10 object must never raise XrefRecovery, got: {warnings:?}"
    );
}
