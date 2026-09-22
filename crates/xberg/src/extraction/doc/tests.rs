use super::*;

#[test]
fn test_cp1252_to_char_ascii() {
    assert_eq!(cp1252_to_char(b'A'), 'A');
    assert_eq!(cp1252_to_char(b' '), ' ');
    assert_eq!(cp1252_to_char(b'\n'), '\n');
}

#[test]
fn test_cp1252_to_char_special() {
    assert_eq!(cp1252_to_char(0x80), '\u{20AC}');
    assert_eq!(cp1252_to_char(0x93), '\u{201C}');
    assert_eq!(cp1252_to_char(0x94), '\u{201D}');
    assert_eq!(cp1252_to_char(0x96), '\u{2013}');
}

#[test]
fn test_normalize_doc_text() {
    assert_eq!(normalize_doc_text("Hello\rWorld"), "Hello\nWorld");
    assert_eq!(normalize_doc_text("A\x07B"), "A\tB");
    assert_eq!(normalize_doc_text("A\x0BB"), "A\nB");
    assert_eq!(normalize_doc_text("A\n\n\n\nB"), "A\n\nB");
}

#[test]
fn test_normalize_doc_text_field_codes() {
    // The instruction between BEGIN and SEPARATOR is markup; only the result survives.
    assert_eq!(normalize_doc_text("A\x13FIELD\x14result\x15B"), "AresultB");
}

#[test]
fn should_drop_hyperlink_instruction_and_keep_result_text() {
    let text = "See \x13 HYPERLINK \"http://example.com/spec\" \\o \"Spec\" \x14the specification\x15 for details.";
    assert_eq!(
        normalize_doc_text(text),
        "See the specification for details.",
        "HYPERLINK instruction must not appear in extracted text"
    );
}

#[test]
fn should_strip_nested_pageref_fields_inside_a_toc_field() {
    // A TOC field whose result contains PAGEREF fields, exactly as Word writes it.
    let text = concat!(
        "\x13 TOC \\o \"1-3\" \\h \\z \\u \x14",
        "\x13 PAGEREF _Toc101 \\h \x141\x15\tIntroduction\n",
        "\x13 PAGEREF _Toc102 \\h \x142\x15\tMethods\n",
        "\x15",
        "Body text."
    );
    assert_eq!(
        normalize_doc_text(text),
        "1\tIntroduction\n2\tMethods\nBody text.",
        "nested PAGEREF/TOC instructions must be stripped without corrupting the result"
    );
}

#[test]
fn should_keep_text_after_an_unterminated_field_begin() {
    // BEGIN with no END at all: treated as inert so the document tail is never lost.
    let text = "Intro.\n\x13PAGEREF _Toc1 \\h \x14";
    assert_eq!(
        normalize_doc_text(text),
        "Intro.\nPAGEREF _Toc1 \\h",
        "an unterminated field must degrade, not swallow the rest of the document"
    );
}

#[test]
fn should_ignore_a_stray_field_end_without_a_begin() {
    assert_eq!(normalize_doc_text("Before\x15After"), "BeforeAfter");
    assert_eq!(
        normalize_doc_text("\x15\x13 SEQ Figure \\* ARABIC \x147\x15\x15Tail"),
        "7Tail",
        "unbalanced END markers must not underflow the field stack"
    );
}

#[test]
fn should_emit_nothing_for_a_terminated_field_without_a_separator() {
    // BEGIN..END with no SEPARATOR: the field has no result, so there is
    // nothing for a reader to see and nothing to emit.
    assert_eq!(
        normalize_doc_text("A\x13 SEQ Figure \\* MERGEFORMAT \x15B"),
        "AB",
        "a resultless field must contribute no text"
    );
}

#[test]
fn should_keep_non_breaking_hyphen_as_a_visible_character() {
    // 0x1E is a hyphen the reader SEES; dropping it welds the compound together.
    assert_eq!(
        normalize_doc_text("Section twenty\x1Eone of the sub\x1Esection"),
        "Section twenty\u{2011}one of the sub\u{2011}section",
        "the non-breaking hyphen is visible text and must not be discarded"
    );
}

#[test]
fn should_keep_non_breaking_hyphen_but_drop_optional_hyphen() {
    // The two are one byte apart and must stay on opposite sides of the line:
    // 0x1E is always rendered, 0x1F only when the line breaks there.
    assert_eq!(
        normalize_doc_text("self\x1Econtained extra\x1Fordinary"),
        "self\u{2011}contained extraordinary",
        "0x1E must survive as U+2011 while 0x1F stays discarded"
    );
}

#[test]
fn should_keep_non_breaking_hyphen_inside_a_field_result() {
    // Field-code stripping runs before character mapping; a cross-reference
    // result such as a clause number must keep its hyphen.
    assert_eq!(
        normalize_doc_text("See \x13 REF _Ref1 \\h \x14clause 3\x1E4\x15."),
        "See clause 3\u{2011}4.",
        "hyphen mapping must apply to text kept from a field result"
    );
}

#[test]
fn test_extract_doc_real_file() {
    let test_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_documents/vendored/unstructured/doc/simple.doc");
    if !test_file.exists() {
        return;
    }
    let content = std::fs::read(&test_file).expect("Failed to read test DOC");
    let result = extract_doc_text(&content).expect("Failed to extract DOC text");
    assert!(!result.content.is_empty(), "DOC extraction should produce text");
}

#[test]
fn test_extract_doc_fake_file() {
    let test_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_documents/vendored/unstructured/doc/fake.doc");
    if !test_file.exists() {
        return;
    }
    let content = std::fs::read(&test_file).expect("Failed to read test DOC");
    let result = extract_doc_text(&content).expect("Failed to extract DOC text");
    assert!(!result.content.is_empty(), "DOC extraction should produce text");
}

#[test]
fn test_extract_doc_invalid_magic() {
    let result = extract_doc_text(b"not a doc file");
    assert!(result.is_err());
}

// --- Synthetic `.doc` byte-fixture helpers for issue #77 / #92 ---
//
// No vendored fixture under `test_documents/` has non-empty
// ccpFtn/ccpAtn or a deliberately-overrunning piece, so these build a
// minimal OLE compound file directly, matching the exact FIB layout this
// module reads (fib_base_size=32, csw=14, cslw=22; see
// `extract_text_word97`). ~keep

const TEST_CSW: usize = 14;
const TEST_CSLW: usize = 22;
const TEST_FIB_BASE: usize = 32;

fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

/// `rg_lw_offset` for the layout built by `build_fib`.
fn test_rg_lw_offset() -> usize {
    let csw_offset = TEST_FIB_BASE;
    let rg_w_offset = csw_offset + 2;
    let cslw_offset = rg_w_offset + TEST_CSW * 2;
    cslw_offset + 2
}

/// `fc_clx_offset` for the layout built by `build_fib` (`lcb_clx_offset`
/// is always `fc_clx_offset + 4`).
fn test_fc_clx_offset() -> usize {
    let rg_lw_offset = test_rg_lw_offset();
    let cbrgfclcb_offset = rg_lw_offset + TEST_CSLW * 4;
    let rg_fc_lcb_offset = cbrgfclcb_offset + 2;
    rg_fc_lcb_offset + FIB_FC_LCB_IDX_CLX * 8
}

/// Build a `len`-byte WordDocument-stream FIB header with the given
/// `ccp*` fields set. `len` must be large enough to hold the header
/// (at least `test_fc_clx_offset() + 8`) plus any text placed after it.
fn build_fib(len: usize, ccp_text: u32, ccp_ftn: u32, ccp_atn: u32, ccp_txbx: u32) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    write_u16(&mut buf, 0, 0xA5EC); // wIdent
    write_u16(&mut buf, 2, 101); // nFib >= 101 selects the Word97+ path ~keep
    write_u16(&mut buf, 0x0A, 0x0200); // fWhichTblStm: use 1Table
    write_u16(&mut buf, TEST_FIB_BASE, TEST_CSW as u16);
    let cslw_offset = TEST_FIB_BASE + 2 + TEST_CSW * 2;
    write_u16(&mut buf, cslw_offset, TEST_CSLW as u16);
    let rg_lw_offset = test_rg_lw_offset();
    write_u32(&mut buf, rg_lw_offset + FIB_LW_IDX_CCP_TEXT * 4, ccp_text);
    write_u32(&mut buf, rg_lw_offset + FIB_LW_IDX_CCP_FTN * 4, ccp_ftn);
    write_u32(&mut buf, rg_lw_offset + FIB_LW_IDX_CCP_ATN * 4, ccp_atn);
    write_u32(&mut buf, rg_lw_offset + FIB_LW_IDX_CCP_TXBX * 4, ccp_txbx);
    buf
}

struct TestPiece {
    cp_start: u32,
    cp_end: u32,
    fc_raw: u32,
}

/// Build a `PlcPcd` (piece table) from a run of contiguous pieces.
fn build_plc_pcd(pieces: &[TestPiece]) -> Vec<u8> {
    let mut buf = Vec::new();
    for p in pieces {
        buf.extend_from_slice(&p.cp_start.to_le_bytes());
    }
    buf.extend_from_slice(&pieces.last().expect("at least one piece").cp_end.to_le_bytes());
    for p in pieces {
        buf.extend_from_slice(&[0u8, 0u8]);
        buf.extend_from_slice(&p.fc_raw.to_le_bytes());
        buf.extend_from_slice(&[0u8, 0u8]);
    }
    buf
}

/// Wire a piece table's `fcClx`/`lcbClx` into `word_doc` and return the
/// matching `1Table`-stream bytes.
fn build_table_stream(word_doc: &mut [u8], plc_pcd: &[u8]) -> Vec<u8> {
    const FC_CLX: u32 = 8;
    let mut clx = vec![0x02u8]; // Pcdt marker
    clx.extend_from_slice(&0u32.to_le_bytes()); // lcb (unused by the reader)
    clx.extend_from_slice(plc_pcd);

    let fc_clx_offset = test_fc_clx_offset();
    write_u32(word_doc, fc_clx_offset, FC_CLX);
    write_u32(word_doc, fc_clx_offset + 4, clx.len() as u32);

    let mut table_stream = vec![0u8; FC_CLX as usize];
    table_stream.extend_from_slice(&clx);
    table_stream
}

/// A compressed (CP1252, 1 byte/char) FC pointing at `byte_offset` in the
/// WordDocument stream.
fn compressed_fc(byte_offset: u32) -> u32 {
    0x4000_0000 | (byte_offset * 2)
}

/// Assemble a minimal `.doc` OLE container from prebuilt streams.
fn build_doc_ole(word_doc: &[u8], table_stream: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut comp = cfb::CompoundFile::create(cursor).expect("create CFB container");
    {
        let mut stream = comp.create_stream("/WordDocument").expect("create WordDocument stream");
        std::io::Write::write_all(&mut stream, word_doc).expect("write WordDocument stream");
    }
    {
        let mut stream = comp.create_stream("/1Table").expect("create 1Table stream");
        std::io::Write::write_all(&mut stream, table_stream).expect("write 1Table stream");
    }
    comp.into_inner().into_inner()
}

/// #77: footnotes, headers, comments and text boxes live in subdocument
/// CP ranges addressed by the FIB's `ccpFtn`/`ccpHdd`/`ccpAtn`/`ccpTxbx`
/// fields. Previously any piece whose CP range started at or after
/// `ccpText` was silently skipped, so this content never appeared.
#[test]
fn test_extract_doc_includes_footnote_and_comment_subdocuments() {
    let main_text = b"Hello";
    let footnote_text = b"Note one";
    let comment_text = b"See me";

    let ccp_text = main_text.len() as u32;
    let ccp_ftn = footnote_text.len() as u32;
    let ccp_atn = comment_text.len() as u32;

    let word_doc_len = 2048;
    let mut word_doc = build_fib(word_doc_len, ccp_text, ccp_ftn, ccp_atn, 0);

    let main_offset = 900usize;
    let footnote_offset = 950usize;
    let comment_offset = 1000usize;
    word_doc[main_offset..main_offset + main_text.len()].copy_from_slice(main_text);
    word_doc[footnote_offset..footnote_offset + footnote_text.len()].copy_from_slice(footnote_text);
    word_doc[comment_offset..comment_offset + comment_text.len()].copy_from_slice(comment_text);

    let pieces = vec![
        TestPiece {
            cp_start: 0,
            cp_end: ccp_text,
            fc_raw: compressed_fc(main_offset as u32),
        },
        TestPiece {
            cp_start: ccp_text,
            cp_end: ccp_text + ccp_ftn,
            fc_raw: compressed_fc(footnote_offset as u32),
        },
        TestPiece {
            cp_start: ccp_text + ccp_ftn,
            cp_end: ccp_text + ccp_ftn + ccp_atn,
            fc_raw: compressed_fc(comment_offset as u32),
        },
    ];
    let plc_pcd = build_plc_pcd(&pieces);
    let table_stream = build_table_stream(&mut word_doc, &plc_pcd);
    let doc_bytes = build_doc_ole(&word_doc, &table_stream);

    let result = extract_doc_text(&doc_bytes).expect("DOC extraction should succeed");

    assert_eq!(result.content, "Hello\n\nFootnotes\n\nNote one\n\nComments\n\nSee me");
    assert!(
        result.processing_warnings.is_empty(),
        "a complete, well-formed document should not warn: {:?}",
        result.processing_warnings
    );
}

/// #92: a piece table entry that declares a byte range past the end of
/// the WordDocument stream must be reported, not silently clamped or
/// dropped.
#[test]
fn test_extract_doc_warns_when_piece_range_overruns_stream() {
    let ccp_text = 10u32;
    let word_doc_len = 700usize;
    let mut word_doc = build_fib(word_doc_len, ccp_text, 0, 0, 0);

    // Only 3 bytes are actually available at this offset; the piece
    // claims 10 compressed (1 byte/char) characters.
    let byte_offset = (word_doc_len - 3) as u32;
    word_doc[word_doc_len - 3..word_doc_len].copy_from_slice(b"Hi!");

    let pieces = vec![TestPiece {
        cp_start: 0,
        cp_end: ccp_text,
        fc_raw: compressed_fc(byte_offset),
    }];
    let plc_pcd = build_plc_pcd(&pieces);
    let table_stream = build_table_stream(&mut word_doc, &plc_pcd);
    let doc_bytes = build_doc_ole(&word_doc, &table_stream);

    let result = extract_doc_text(&doc_bytes).expect("DOC extraction should succeed despite the overrun");

    assert_eq!(
        result.content, "Hi!",
        "should keep the bytes that ARE available, dropping only the overrun tail"
    );
    assert_eq!(result.processing_warnings.len(), 1);
    assert_eq!(result.processing_warnings[0].source, "doc");
    assert!(
        result.processing_warnings[0]
            .message
            .contains("past the end of the WordDocument stream"),
        "warning should name the overrun: {:?}",
        result.processing_warnings[0].message
    );
}

/// #1551: `fcClx` was read from `FibRgFcLcb97` pair 66 (`fcBkdFtnOldOld`,
/// an obsolete field Word writes as zero) instead of pair 33. `fc_clx == 0`
/// therefore held for every real document, the piece table was never walked,
/// and extraction silently fell back to reading `reserved5`/`reserved6` at
/// `0x18`/`0x1C` -- bytes [MS-DOC] says a reader must ignore.
///
/// The pair index is written here as a literal rather than through
/// [`FIB_FC_LCB_IDX_CLX`], deliberately. Both the reader and `build_fib`'s
/// helper use that constant, so a test that positioned the `Clx` through the
/// helper would move with a regression and stay green -- which is exactly why
/// the original defect survived a suite that already covered the piece table.
/// Pinning 33 independently is what makes this guard able to fail. ~keep
#[test]
fn fc_clx_is_read_at_ms_doc_pair_33_not_the_obsolete_pair_66() {
    const MS_DOC_SPEC_FC_CLX_PAIR: usize = 33;
    const OBSOLETE_PAIR_THE_READER_USED_TO_USE: usize = 66;
    const TEXT: &str = "lorem ipsum dolor sit amet";
    /// Placed where the contiguous fallback looks, so the two paths cannot be
    /// confused for one another: whichever string comes back names the path
    /// that ran. ~keep
    const FALLBACK_DECOY: &str = "FALLBACK DECOY TEXT NOT THE DOCUMENT BODY";
    const TEXT_OFFSET: usize = 2048;
    const DECOY_OFFSET: usize = 1536;

    let mut word_doc = build_fib(TEXT_OFFSET + TEXT.len(), TEXT.len() as u32, 0, 0, 0);
    word_doc[TEXT_OFFSET..TEXT_OFFSET + TEXT.len()].copy_from_slice(TEXT.as_bytes());
    word_doc[DECOY_OFFSET..DECOY_OFFSET + FALLBACK_DECOY.len()].copy_from_slice(FALLBACK_DECOY.as_bytes());

    // reserved5/reserved6 -- what the fallback reads as fcMin/fcMac.
    write_u32(&mut word_doc, 0x18, DECOY_OFFSET as u32);
    write_u32(&mut word_doc, 0x1C, (DECOY_OFFSET + FALLBACK_DECOY.len()) as u32);

    let plc_pcd = build_plc_pcd(&[TestPiece {
        cp_start: 0,
        cp_end: TEXT.len() as u32,
        fc_raw: compressed_fc(TEXT_OFFSET as u32),
    }]);

    const FC_CLX: u32 = 8;
    let mut clx = vec![0x02u8];
    clx.extend_from_slice(&0u32.to_le_bytes());
    clx.extend_from_slice(&plc_pcd);

    let rg_fc_lcb_offset = test_rg_lw_offset() + TEST_CSLW * 4 + 2;
    let spec_pair = rg_fc_lcb_offset + MS_DOC_SPEC_FC_CLX_PAIR * 8;
    write_u32(&mut word_doc, spec_pair, FC_CLX);
    write_u32(&mut word_doc, spec_pair + 4, clx.len() as u32);

    let obsolete_pair = rg_fc_lcb_offset + OBSOLETE_PAIR_THE_READER_USED_TO_USE * 8;
    assert_eq!(
        u32::from_le_bytes(word_doc[obsolete_pair..obsolete_pair + 4].try_into().expect("4 bytes")),
        0,
        "pair 66 must stay zero -- it is what every real document holds, and the defect \
         was invisible precisely because reading it yields 0"
    );

    let mut table_stream = vec![0u8; FC_CLX as usize];
    table_stream.extend_from_slice(&clx);

    let doc_bytes = build_doc_ole(&word_doc, &table_stream);
    let result = extract_doc_text(&doc_bytes).expect("DOC extraction should succeed");

    assert_eq!(
        result.content, TEXT,
        "text must come from the piece table at pair 33; got {:?}",
        result.content
    );
    assert!(
        !result.content.contains("FALLBACK DECOY"),
        "the contiguous fallback ran, so fcClx read as 0: {:?}",
        result.content
    );
}
