use super::*;
use std::io::Write;

/// Build a minimal ZIP in memory with one file at the given path and contents.
fn make_zip_with_file(entry_path: &str, entry_data: &[u8]) -> Vec<u8> {
    make_zip_with_files(&[(entry_path, entry_data)])
}

/// Build a minimal ZIP in memory with several files at the given paths and contents.
fn make_zip_with_files(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = zip::write::FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    for (entry_path, entry_data) in entries {
        zip.start_file(*entry_path, options).unwrap();
        zip.write_all(entry_data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Bit-by-bit CRC-32 (IEEE 802.3 / zlib / ZIP polynomial 0xEDB88320).
///
/// The hand-forged archive below cannot go through `zip::ZipWriter` (it needs a
/// central-directory uncompressed-size that the writer's public API has no way to
/// misstate), so the CRC the reader checks at end-of-stream has to be computed here
/// too, matching exactly what any standard ZIP implementation would produce.
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Hand-construct a single-entry ZIP archive whose central-directory record declares an
/// enormous uncompressed size via a Zip64 extended-information extra field, while the
/// real stored payload (and the compressed-size field that bounds the actual read) stays
/// tiny.
///
/// This forges exactly the shape described for the vulnerability: `zip::ZipWriter`'s
/// public API has no method to write a declared size that disagrees with the real
/// payload, so the archive is built byte-by-byte instead, matching the `zip` crate's own
/// on-disk layout (`ZipLocalEntryBlock`, `ZipCentralEntryBlock`, the Zip64 extended-info
/// extra field, and `Zip32CDEBlock`/EOCD -- see `zip-8.6.0/src/spec.rs` and
/// `zip-8.6.0/src/extra_fields/zip64_extended_information.rs`).
///
/// The central-directory `uncompressed_size` 32-bit field is set to the ZIP64 sentinel
/// (`0xFFFFFFFF`), which the reader ignores in favor of an 8-byte Zip64 extra field
/// carrying `forged_uncompressed_size`. The `compressed_size` field is left at the real,
/// honest payload length -- the reader's `find_content` bounds the *actual* on-disk read
/// to `compressed_size`, so this is what makes the entry parse and read successfully at
/// all despite the forged size, exactly like a forged real-world OOXML attachment would.
fn make_forged_zip64_entry(entry_name: &str, payload: &[u8], forged_uncompressed_size: u64) -> Vec<u8> {
    let name_bytes = entry_name.as_bytes();
    let crc = crc32_ieee(payload);
    let compressed_size = payload.len() as u32;

    let mut out = Vec::new();

    // -- Local File Header (ZipLocalEntryBlock, spec.rs) --
    let local_header_start = out.len() as u32;
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local file header signature
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed to extract
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // compression method: Stored
    out.extend_from_slice(&0u16.to_le_bytes()); // last mod time
    out.extend_from_slice(&0u16.to_le_bytes()); // last mod date
    out.extend_from_slice(&crc.to_le_bytes()); // crc32
    out.extend_from_slice(&compressed_size.to_le_bytes()); // compressed size (honest)
    out.extend_from_slice(&compressed_size.to_le_bytes()); // uncompressed size (local; unused by the reader)
    out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // file name length
    out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
    out.extend_from_slice(name_bytes);
    // -- file data (Stored, verbatim) --
    out.extend_from_slice(payload);

    // -- Zip64 extended-information extra field (only the uncompressed-size slot is
    // populated; kept under 24 bytes so the reader's parser does not also expect a
    // compressed-size or header-start slot to follow) --
    let mut zip64_extra = Vec::new();
    zip64_extra.extend_from_slice(&0x0001u16.to_le_bytes()); // Zip64 extended info tag
    zip64_extra.extend_from_slice(&8u16.to_le_bytes()); // this field's data length: one u64
    zip64_extra.extend_from_slice(&forged_uncompressed_size.to_le_bytes());
    assert_eq!(zip64_extra.len(), 12);

    // -- Central Directory File Header (ZipCentralEntryBlock, spec.rs) --
    let central_header_start = out.len() as u32;
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes()); // central file header signature
    out.extend_from_slice(&45u16.to_le_bytes()); // version made by (45 = zip64 support)
    out.extend_from_slice(&45u16.to_le_bytes()); // version needed to extract
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // compression method: Stored
    out.extend_from_slice(&0u16.to_le_bytes()); // last mod time
    out.extend_from_slice(&0u16.to_le_bytes()); // last mod date
    out.extend_from_slice(&crc.to_le_bytes()); // crc32
    out.extend_from_slice(&compressed_size.to_le_bytes()); // compressed size (honest)
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // uncompressed size: ZIP64 sentinel
    out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // file name length
    out.extend_from_slice(&(zip64_extra.len() as u16).to_le_bytes()); // extra field length
    out.extend_from_slice(&0u16.to_le_bytes()); // file comment length
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // internal file attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // external file attributes
    out.extend_from_slice(&local_header_start.to_le_bytes()); // relative offset of local header
    out.extend_from_slice(name_bytes);
    out.extend_from_slice(&zip64_extra);

    let central_directory_size = out.len() as u32 - central_header_start;

    // -- End Of Central Directory record (Zip32CDEBlock, spec.rs) --
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes()); // EOCD signature
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with central directory
    out.extend_from_slice(&1u16.to_le_bytes()); // number of files on this disk
    out.extend_from_slice(&1u16.to_le_bytes()); // total number of files
    out.extend_from_slice(&central_directory_size.to_le_bytes());
    out.extend_from_slice(&central_header_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length

    out
}

/// Bytes with no recognizable magic and no valid UTF-8, so both the byte-sniffing and
/// extension-based MIME fallbacks fail deterministically regardless of which optional
/// extractor features are compiled in. Used to make "how many embeddings were processed"
/// observable purely by counting "MIME type could not be determined" warnings.
const UNDETECTABLE_MIME_BYTES: &[u8] = &[0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87];

#[tokio::test]
async fn test_embedded_file_over_cap_skipped_with_warning() {
    let data = b"Hello world! This is a test document.";
    let zip_bytes = make_zip_with_file("word/embeddings/doc.txt", data);

    let config = ExtractionConfig {
        max_embedded_file_bytes: Some(10),
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(
        children.is_empty(),
        "oversized embedded file must not produce a child entry"
    );
    assert_eq!(warnings.len(), 1, "exactly one warning expected");
    assert!(
        warnings[0].message.contains("exceeds cap"),
        "warning must mention cap: {}",
        warnings[0].message
    );
    assert!(
        warnings[0].message.contains("doc.txt"),
        "warning must name the file: {}",
        warnings[0].message
    );
}

#[tokio::test]
async fn test_embedded_file_under_cap_proceeds_to_extraction() {
    let data = b"Hello";
    let zip_bytes = make_zip_with_file("word/embeddings/note.txt", data);

    let config = ExtractionConfig {
        max_embedded_file_bytes: Some(1024 * 1024),
        ..Default::default()
    };

    let (_children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    let cap_warnings: Vec<_> = warnings.iter().filter(|w| w.message.contains("exceeds cap")).collect();
    assert!(cap_warnings.is_empty(), "no size-cap warning expected for small file");
}

#[tokio::test]
async fn test_embedded_file_no_cap_proceeds() {
    let data = b"some content";
    let zip_bytes = make_zip_with_file("word/embeddings/file.txt", data);

    let config = ExtractionConfig {
        max_embedded_file_bytes: None,
        ..Default::default()
    };

    let (_children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    let cap_warnings: Vec<_> = warnings.iter().filter(|w| w.message.contains("exceeds cap")).collect();
    assert!(cap_warnings.is_empty(), "no size-cap warning when cap is None");
}

/// Build a CFB (OLE compound file) with a single "Package" stream holding `payload`,
/// the shape OLE object wrappers use to embed a modern Office (OPC/ZIP) document
/// verbatim.
// Only consumer is `test_ole_package_stream_extracted_as_embedded_xlsx`, which is
// `#[cfg(feature = "excel")]` for the reason documented on it. Matching that gate here
// keeps an `office`-without-`excel` build warning-free.
#[cfg(feature = "excel")]
fn make_ole_package(payload: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut comp = cfb::CompoundFile::create(cursor).expect("create CFB container");
    {
        let mut stream = comp.create_stream("Package").expect("create Package stream");
        stream.write_all(payload).unwrap();
    }
    comp.into_inner().into_inner()
}

/// Build a CFB with a single named stream (e.g. "WordDocument"), simulating a legacy
/// binary Office document embedded directly as an OLE compound file.
fn make_ole_with_stream(stream_name: &str, payload: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut comp = cfb::CompoundFile::create(cursor).expect("create CFB container");
    {
        let mut stream = comp.create_stream(stream_name).expect("create stream");
        stream.write_all(payload).unwrap();
    }
    comp.into_inner().into_inner()
}

/// Path to the shared `test_documents/` corpus (two levels up from this crate).
#[cfg(feature = "excel")]
fn test_documents_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test_documents")
}

/// Gated on `excel` as well as `office`: the payload is an .xlsx, so without the
/// excel extractor registered the recursive extraction correctly reports
/// `UnsupportedFormat` and produces no child. The test would then fail for a reason
/// that has nothing to do with OLE Package unwrapping, which is what it exists to
/// cover. Observed under `--features "email,office,ocr,transcription"`.
#[cfg(feature = "excel")]
#[tokio::test]
async fn test_ole_package_stream_extracted_as_embedded_xlsx() {
    let fixture = test_documents_dir().join("xlsx/excel_tiny_excel.xlsx");
    if !fixture.exists() {
        eprintln!(
            "Skipping test: test_documents/ fixture not found at {}",
            fixture.display()
        );
        return;
    }
    let xlsx_bytes = std::fs::read(&fixture).expect("read fixture xlsx");

    let ole_bytes = make_ole_package(&xlsx_bytes);
    let zip_bytes = make_zip_with_file("word/embeddings/oleObject1.bin", &ole_bytes);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert_eq!(
        children.len(),
        1,
        "the OLE Package stream must be unwrapped and recursively extracted; warnings: {:?}",
        warnings
    );
    assert!(
        children[0].mime_type.contains("spreadsheet") || children[0].mime_type.contains("excel"),
        "expected an Excel MIME type, got '{}'",
        children[0].mime_type
    );
}

#[tokio::test]
async fn test_legacy_word_document_ole_stream_is_identified_not_skipped() {
    // The WordDocument content doesn't need to be a well-formed FIB for this test: we
    // only assert that the OLE container was recognized and routed to the legacy
    // `.doc` MIME type instead of being reported as unidentifiable outright. Whether
    // the FIB itself parses is covered by `extraction::doc` tests.
    let ole_bytes = make_ole_with_stream("WordDocument", b"not-a-real-fib-but-present");
    let zip_bytes = make_zip_with_file("word/embeddings/oleObject2.bin", &ole_bytes);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty());
    assert_eq!(warnings.len(), 1, "expected exactly one warning: {:?}", warnings);
    assert!(
        !warnings[0].message.contains("format identification not supported"),
        "a recognized WordDocument stream must not be reported as unidentifiable: {}",
        warnings[0].message
    );
}

#[tokio::test]
async fn test_unidentifiable_ole_container_still_warns() {
    let ole_bytes = make_ole_with_stream("SomeUnknownStream", b"opaque binary data");
    let zip_bytes = make_zip_with_file("word/embeddings/oleObject3.bin", &ole_bytes);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("format identification not supported"));
    assert!(warnings[0].message.contains("oleObject3.bin"));
}

#[tokio::test]
async fn test_undetectable_mime_now_warns_instead_of_silent_skip() {
    // Bytes with no recognizable magic, invalid as UTF-8 (so the plain-text fallback
    // doesn't kick in either), and no file extension: MIME detection must fail for
    // both the byte-sniffing and extension-based fallback paths.
    let data = vec![0x80u8, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87];
    let zip_bytes = make_zip_with_file("word/embeddings/mystery_blob", &data);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty());
    assert_eq!(warnings.len(), 1, "expected exactly one warning: {:?}", warnings);
    assert!(
        warnings[0].message.contains("mystery_blob"),
        "warning must name the file: {}",
        warnings[0].message
    );
    assert!(
        warnings[0].message.contains("MIME type could not be determined"),
        "warning must explain why the file was skipped: {}",
        warnings[0].message
    );
}

#[tokio::test]
async fn test_depth_exhausted_with_embeddings_present_warns() {
    let data = b"Hello world! This is a test document.";
    let zip_bytes = make_zip_with_file("word/embeddings/doc.txt", data);

    let config = ExtractionConfig {
        max_archive_depth: 0,
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty());
    assert_eq!(warnings.len(), 1, "expected exactly one warning: {:?}", warnings);
    assert!(
        warnings[0].message.contains("max_archive_depth"),
        "warning must explain why embeddings were skipped: {}",
        warnings[0].message
    );
}

#[tokio::test]
async fn test_depth_exhausted_with_no_embeddings_does_not_warn() {
    let zip_bytes = make_zip_with_file("word/document.xml", b"<w:document/>");

    let config = ExtractionConfig {
        max_archive_depth: 0,
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty());
    assert!(
        warnings.is_empty(),
        "no embeddings exist, so no depth warning should be emitted: {:?}",
        warnings
    );
}

#[tokio::test]
async fn test_embedded_objects_exceeding_max_files_in_archive_are_rejected() {
    // 5 embedded entries, each undetectable by MIME so every processed entry produces
    // exactly one "MIME type could not be determined" warning. Unfixed code reads no
    // count limit at all, so it would process and warn on all 5; the fixed code must
    // stop after `max_files_in_archive` (2) and report the remaining 3 as skipped.
    let entries: Vec<(String, Vec<u8>)> = (0..5)
        .map(|i| (format!("word/embeddings/blob{i}"), UNDETECTABLE_MIME_BYTES.to_vec()))
        .collect();
    let entry_refs: Vec<(&str, &[u8])> = entries.iter().map(|(p, d)| (p.as_str(), d.as_slice())).collect();
    let zip_bytes = make_zip_with_files(&entry_refs);

    let config = ExtractionConfig {
        security_limits: Some(crate::extractors::security::SecurityLimits {
            max_files_in_archive: 2,
            ..Default::default()
        }),
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty(), "undetectable-MIME entries never produce children");

    let cap_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("max_files_in_archive"))
        .collect();
    assert_eq!(
        cap_warnings.len(),
        1,
        "expected exactly one cap warning: {:?}",
        warnings
    );
    assert!(
        cap_warnings[0].message.contains("Skipped 3"),
        "warning must report the 3 skipped entries: {}",
        cap_warnings[0].message
    );
    assert!(
        cap_warnings[0].message.contains("max_files_in_archive (2)"),
        "warning must name the limit that was hit: {}",
        cap_warnings[0].message
    );

    let processed_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("MIME type could not be determined"))
        .collect();
    assert_eq!(
        processed_warnings.len(),
        2,
        "only max_files_in_archive (2) entries must be processed, not all 5: {:?}",
        warnings
    );
}

#[tokio::test]
async fn test_embedded_objects_just_under_max_files_in_archive_all_process() {
    // 4 entries against a cap of 5: every entry must still be attempted and no cap
    // warning should fire. A fix that rejects everything (e.g. off-by-one, or clamping
    // to 0) would fail this.
    let entries: Vec<(String, Vec<u8>)> = (0..4)
        .map(|i| (format!("word/embeddings/blob{i}"), UNDETECTABLE_MIME_BYTES.to_vec()))
        .collect();
    let entry_refs: Vec<(&str, &[u8])> = entries.iter().map(|(p, d)| (p.as_str(), d.as_slice())).collect();
    let zip_bytes = make_zip_with_files(&entry_refs);

    let config = ExtractionConfig {
        security_limits: Some(crate::extractors::security::SecurityLimits {
            max_files_in_archive: 5,
            ..Default::default()
        }),
        ..Default::default()
    };

    let (_children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    let cap_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("max_files_in_archive"))
        .collect();
    assert!(
        cap_warnings.is_empty(),
        "no cap warning expected when the count is under the limit: {:?}",
        warnings
    );

    let processed_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("MIME type could not be determined"))
        .collect();
    assert_eq!(
        processed_warnings.len(),
        4,
        "all 4 entries must be processed when under the cap: {:?}",
        warnings
    );
}

#[tokio::test]
async fn test_legitimate_document_under_max_files_in_archive_extracts_successfully() {
    // A real, extractable payload (plain text) under the cap must still produce a
    // child entry — proving the fix does not merely suppress warnings but leaves
    // legitimate extraction intact.
    let zip_bytes = make_zip_with_file("word/embeddings/note.txt", b"Hello, world!");

    let config = ExtractionConfig {
        security_limits: Some(crate::extractors::security::SecurityLimits {
            max_files_in_archive: 10,
            ..Default::default()
        }),
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert_eq!(
        children.len(),
        1,
        "the single embedded file, well under the cap, must be extracted: {:?}",
        warnings
    );
    assert!(
        !warnings.iter().any(|w| w.message.contains("max_files_in_archive")),
        "no cap warning expected: {:?}",
        warnings
    );
}

#[tokio::test]
async fn test_nested_container_enforces_max_files_in_archive_independently() {
    // Per-container accounting: `extract_ooxml_embedded_objects` is invoked once per
    // container (the outer DOCX/PPTX/XLSX, and again recursively for any embedded
    // OOXML container found inside it, via `extract_bytes`). This test proves the cap
    // is applied fresh to each container's own embeddings directory rather than
    // decremented from some shared, cumulative counter: an outer container with 2
    // embeddings (under a cap of 2) and, independently, an inner container with 5
    // embeddings (over the same cap of 2) each get judged solely against their own
    // entry count.
    let outer_entries: Vec<(String, Vec<u8>)> = (0..2)
        .map(|i| (format!("word/embeddings/outer{i}"), UNDETECTABLE_MIME_BYTES.to_vec()))
        .collect();
    let outer_refs: Vec<(&str, &[u8])> = outer_entries.iter().map(|(p, d)| (p.as_str(), d.as_slice())).collect();
    let outer_zip_bytes = make_zip_with_files(&outer_refs);

    let inner_entries: Vec<(String, Vec<u8>)> = (0..5)
        .map(|i| (format!("word/embeddings/inner{i}"), UNDETECTABLE_MIME_BYTES.to_vec()))
        .collect();
    let inner_refs: Vec<(&str, &[u8])> = inner_entries.iter().map(|(p, d)| (p.as_str(), d.as_slice())).collect();
    let inner_zip_bytes = make_zip_with_files(&inner_refs);

    let config = ExtractionConfig {
        security_limits: Some(crate::extractors::security::SecurityLimits {
            max_files_in_archive: 2,
            ..Default::default()
        }),
        ..Default::default()
    };

    let (_outer_children, outer_warnings) =
        extract_ooxml_embedded_objects(&outer_zip_bytes, "word/embeddings/", "outer", &config).await;
    let (_inner_children, inner_warnings) =
        extract_ooxml_embedded_objects(&inner_zip_bytes, "word/embeddings/", "inner", &config).await;

    assert!(
        !outer_warnings
            .iter()
            .any(|w| w.message.contains("max_files_in_archive")),
        "outer container is exactly at the cap and must not warn: {:?}",
        outer_warnings
    );
    let inner_cap_warnings: Vec<_> = inner_warnings
        .iter()
        .filter(|w| w.message.contains("max_files_in_archive"))
        .collect();
    assert_eq!(
        inner_cap_warnings.len(),
        1,
        "inner container independently exceeds the same cap: {:?}",
        inner_warnings
    );
    assert!(
        inner_cap_warnings[0].message.contains("Skipped 3"),
        "inner container's own 5 entries against a cap of 2 must skip 3: {}",
        inner_cap_warnings[0].message
    );
}

/// Direct, allocation-free test of the clamp itself: a forged multi-terabyte declared
/// size (an attacker-controlled ZIP central-directory uncompressed-size field) must be
/// clamped down to the configured cap, never passed through as-is.
#[test]
fn test_clamp_declared_size_bounds_forged_declaration_to_cap() {
    let forged_declared_size = 4u64 * 1024 * 1024 * 1024 * 1024; // 4 TiB
    let cap = 50 * 1024 * 1024; // the default max_embedded_file_bytes
    assert_eq!(
        clamp_declared_size(forged_declared_size, cap),
        cap,
        "a forged multi-terabyte declared size must be clamped to the configured cap"
    );
}

/// Boundary: a declared size exactly at the cap must pass through unchanged (proves the
/// clamp isn't off-by-one and doesn't needlessly shrink a legitimately-sized file).
#[test]
fn test_clamp_declared_size_passes_through_value_at_cap() {
    let cap = 50 * 1024 * 1024;
    assert_eq!(clamp_declared_size(cap, cap), cap);
}

/// An honest, small declared size well under the cap must pass through unchanged.
#[test]
fn test_clamp_declared_size_passes_through_honest_value_under_cap() {
    let cap = 50 * 1024 * 1024;
    assert_eq!(clamp_declared_size(1024, cap), 1024);
}

/// End-to-end reproduction of the vulnerability: a DOCX embedding whose ZIP
/// central-directory record declares an uncompressed size of `u64::MAX` (via a forged
/// Zip64 extended-information extra field) while the real stored payload is a few bytes.
///
/// `u64::MAX` is deliberately chosen over a merely large value like "4 TB": on unfixed
/// code (`Vec::with_capacity(file.size() as usize)`), any capacity request whose byte
/// count exceeds `isize::MAX` makes `Vec::with_capacity` panic with "capacity overflow"
/// -- unconditionally, on any platform, regardless of available RAM or virtual-memory
/// overcommit settings. A merely-large-but-representable value (a few TB) would not give
/// this guarantee: 64-bit operating systems can often satisfy a multi-terabyte
/// `with_capacity` as a lazy virtual-memory reservation without touching a single page,
/// so such a test could pass "by accident" on unfixed code and prove nothing. Choosing a
/// declared size just past `isize::MAX` instead makes the unfixed behavior a deterministic
/// panic (this `#[tokio::test]` would fail with "capacity overflow"), not a
/// platform-dependent maybe-OOM-maybe-not.
///
/// Against the fixed code, `clamp_declared_size` bounds the allocation hint to
/// `embedded_capacity_cap` (here the default 50 MiB) before `Vec::with_capacity` is ever
/// called, so no such request is made; the tiny real payload is read normally and (being
/// undetectable-MIME junk) is reported exactly like any other unidentifiable embedding.
#[tokio::test]
async fn test_forged_multi_terabyte_declared_size_does_not_overflow_allocation() {
    let zip_bytes = make_forged_zip64_entry("word/embeddings/huge.bin", UNDETECTABLE_MIME_BYTES, u64::MAX);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(
        children.is_empty(),
        "undetectable-MIME entry must never produce a child: {:?}",
        children.len()
    );
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one warning, proving the entry was read and processed rather \
         than rejected outright: {:?}",
        warnings
    );
    assert!(
        warnings[0].message.contains("huge.bin"),
        "warning must name the file: {}",
        warnings[0].message
    );
    assert!(
        warnings[0].message.contains("MIME type could not be determined"),
        "the tiny real payload must reach the normal MIME-detection path, not be rejected \
         for its (forged) declared size: {}",
        warnings[0].message
    );
}

/// Same forged declaration, but the real payload is legitimate small text. Proves the fix
/// doesn't merely avoid crashing -- the embedding is still correctly extracted, with its
/// real content intact, despite the archive's central directory lying about its size.
#[tokio::test]
async fn test_forged_declared_size_still_extracts_real_small_payload() {
    let payload = b"Hello, world!";
    let zip_bytes = make_forged_zip64_entry("word/embeddings/note.txt", payload, u64::MAX);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert_eq!(
        children.len(),
        1,
        "the real (small) payload behind the forged declaration must still be extracted: {:?}",
        warnings
    );
    assert_eq!(
        children[0].result.content.trim(),
        "Hello, world!",
        "extracted content must match the real payload bytes, not be corrupted by the \
         forged declared size"
    );
}

/// Positive control: an ordinary embedded object (no forged metadata at all) with a real
/// small payload must extract with exactly the same content as before this fix -- proving
/// the clamp does not affect legitimate, honestly-declared embeddings.
#[tokio::test]
async fn test_legitimate_small_embedded_object_extracts_unchanged_bytes() {
    let payload = b"Hello, world!";
    let zip_bytes = make_zip_with_file("word/embeddings/note.txt", payload);

    let config = ExtractionConfig::default();
    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert_eq!(
        children.len(),
        1,
        "a legitimate small embedded file must still be extracted: {:?}",
        warnings
    );
    assert!(warnings.is_empty(), "no warnings expected: {:?}", warnings);
    assert_eq!(
        children[0].result.content.trim(),
        "Hello, world!",
        "extracted content must be exactly the original bytes"
    );
}

/// Boundary: a real (honest) embedded file whose size is exactly at the configured cap
/// must be extracted, not rejected. Proves the `> cap` comparison (not `>=`).
#[tokio::test]
async fn test_embedded_file_exactly_at_cap_is_extracted() {
    let payload = b"Hello, world!"; // 13 bytes
    let zip_bytes = make_zip_with_file("word/embeddings/note.txt", payload);

    let config = ExtractionConfig {
        max_embedded_file_bytes: Some(payload.len() as u64),
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(
        !warnings.iter().any(|w| w.message.contains("exceeds cap")),
        "a file exactly at the cap must not be treated as oversized: {:?}",
        warnings
    );
    assert_eq!(
        children.len(),
        1,
        "a file exactly at the cap must still be extracted: {:?}",
        warnings
    );
}

/// Boundary: one byte over the configured cap must be rejected with the size-exceeded
/// warning and produce no child.
#[tokio::test]
async fn test_embedded_file_one_byte_over_cap_is_rejected() {
    let payload = b"Hello, world!!"; // 14 bytes
    let zip_bytes = make_zip_with_file("word/embeddings/note.txt", payload);

    let config = ExtractionConfig {
        max_embedded_file_bytes: Some((payload.len() - 1) as u64),
        ..Default::default()
    };

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(
        children.is_empty(),
        "a file one byte over the cap must not produce a child"
    );
    assert_eq!(warnings.len(), 1, "expected exactly one warning: {:?}", warnings);
    assert!(
        warnings[0].message.contains("exceeds cap"),
        "warning must mention the cap: {}",
        warnings[0].message
    );
}

#[tokio::test]
async fn test_embedded_objects_fall_back_to_default_max_files_in_archive_when_unset() {
    // `security_limits: None` must mean "the `SecurityLimits` default", not "no limit".
    // One entry past the default ceiling must be skipped and reported. The entries are
    // empty so the loop skips each processed one before extraction; the test costs one
    // ZIP central directory, not ten thousand extractions.
    let default_limit = crate::extractors::security::SecurityLimits::default().max_files_in_archive;
    let entries: Vec<String> = (0..=default_limit)
        .map(|i| format!("word/embeddings/blob{i}"))
        .collect();
    let entry_refs: Vec<(&str, &[u8])> = entries.iter().map(|p| (p.as_str(), &[][..])).collect();
    let zip_bytes = make_zip_with_files(&entry_refs);

    let config = ExtractionConfig::default();
    assert!(
        config.security_limits.is_none(),
        "this test must exercise the unset fallback, not an explicit limit"
    );

    let (children, warnings) = extract_ooxml_embedded_objects(&zip_bytes, "word/embeddings/", "test", &config).await;

    assert!(children.is_empty(), "empty entries never produce children");
    assert_eq!(
        warnings.len(),
        1,
        "exactly one cap warning expected, nothing else: {:?}",
        warnings
    );
    assert!(
        warnings[0].message.contains("Skipped 1 "),
        "exactly one entry past the default ceiling must be skipped: {}",
        warnings[0].message
    );
    assert!(
        warnings[0]
            .message
            .contains(&format!("max_files_in_archive ({default_limit})")),
        "warning must name the default limit that was hit: {}",
        warnings[0].message
    );
}
