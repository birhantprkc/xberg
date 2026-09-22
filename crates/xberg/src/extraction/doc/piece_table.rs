//! Word97+ piece-table (`PlcPcd`) decoding: FIB field reads, per-piece text
//! decoding, and assembly into the document's subdocument text ranges.

use super::*;

/// Read a little-endian `u16` at `offset`, or an error naming `what` when `data` is too
/// short to contain it.
fn read_u16_checked(data: &[u8], offset: usize, what: &str) -> Result<u16> {
    if data.len() < offset + 2 {
        return Err(XbergError::parsing(format!("FIB too short for {what}")));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read a little-endian `u32` at `offset`, or an error naming `what` when `data` is too
/// short to contain it.
fn read_u32_checked(data: &[u8], offset: usize, what: &str) -> Result<u32> {
    if data.len() < offset + 4 {
        return Err(XbergError::parsing(format!("FIB too short for {what}")));
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Extract text from Word 97/2000/XP/2003 files using the piece table.
pub(super) fn extract_text_word97(
    word_doc: &[u8],
    table_stream: &[u8],
    warnings: &mut Vec<ProcessingWarning>,
) -> Result<MainText> {
    let fib_base_size = 32;
    let csw_offset = fib_base_size;
    let csw = read_u16_checked(word_doc, csw_offset, "csw")? as usize;
    let rg_w_offset = csw_offset + 2;
    let cslw_offset = rg_w_offset + csw * 2;

    let cslw = read_u16_checked(word_doc, cslw_offset, "cslw")? as usize;
    let rg_lw_offset = cslw_offset + 2;

    let ccp_text_offset = rg_lw_offset + FIB_LW_IDX_CCP_TEXT * 4;
    let ccp_text = read_u32_checked(word_doc, ccp_text_offset, "ccpText")? as usize;

    let subdoc_ranges = SubdocRanges::from_fib(word_doc, rg_lw_offset, ccp_text);
    let mut total_cp = subdoc_ranges.total_cp();
    if total_cp > 0 {
        total_cp += 1;
    }

    let cbrgfclcb_offset = rg_lw_offset + cslw * 4;
    read_u16_checked(word_doc, cbrgfclcb_offset, "cbRgFcLcb")?;
    let rg_fc_lcb_offset = cbrgfclcb_offset + 2;

    let fc_clx_offset = rg_fc_lcb_offset + FIB_FC_LCB_IDX_CLX * 8;
    let lcb_clx_offset = fc_clx_offset + 4;
    let fc_clx = read_u32_checked(word_doc, fc_clx_offset, "fcClx/lcbClx")? as usize;
    let lcb_clx = read_u32_checked(word_doc, lcb_clx_offset, "fcClx/lcbClx")? as usize;

    if fc_clx == 0 || lcb_clx == 0 {
        return extract_text_contiguous(word_doc, ccp_text).map(MainText::text_only);
    }

    if table_stream.len() < fc_clx + lcb_clx {
        return Err(XbergError::parsing("CLX extends beyond table stream"));
    }

    let clx = &table_stream[fc_clx..fc_clx + lcb_clx];

    let mut pos = 0;
    while pos < clx.len() {
        let clxt = clx[pos];
        if clxt == 0x02 {
            pos += 1;
            if pos + 4 > clx.len() {
                return Err(XbergError::parsing("Pcdt truncated at lcb"));
            }
            let _ = u32::from_le_bytes([clx[pos], clx[pos + 1], clx[pos + 2], clx[pos + 3]]) as usize;
            pos += 4;

            let plc_pcd = &clx[pos..];
            let list_tables = papx::ListTables::build(word_doc, table_stream, rg_fc_lcb_offset);
            return extract_text_from_piece_table(word_doc, plc_pcd, &subdoc_ranges, total_cp, warnings, &list_tables);
        } else if clxt == 0x01 {
            pos += 1;
            if pos + 2 > clx.len() {
                break;
            }
            let cb_grpprl = u16::from_le_bytes([clx[pos], clx[pos + 1]]) as usize;
            pos += 2 + cb_grpprl;
        } else {
            break;
        }
    }

    extract_text_fallback(word_doc, ccp_text).map(MainText::text_only)
}

/// Record that a piece's declared byte range runs past the end of the
/// WordDocument stream (#92). The piece table on a malformed or truncated
/// `.doc` can declare an FC/length pair that overruns the stream; previously
/// this was silently clamped (compressed pieces) or dropped entirely
/// (uncompressed pieces), producing a document that looked complete but was
/// truncated.
fn push_piece_overrun_warning(
    warnings: &mut Vec<ProcessingWarning>,
    piece_index: usize,
    declared_end: usize,
    stream_len: usize,
) {
    let message = format!(
        "Piece {piece_index} in the .doc piece table declares a byte range ending at \
         byte {declared_end}, past the end of the WordDocument stream ({stream_len} bytes); \
         the piece's text beyond the stream end was dropped"
    );
    crate::core::diagnostics::push_warning(warnings, DOC_WARNING_SOURCE, message);
}

/// Decode up to `char_count` characters for one piece-table piece, mirroring
/// the compressed (CP1252, 1 byte/char) vs. uncompressed (UTF-16LE, 2
/// bytes/char) layouts used by the legacy `.doc` piece table, plus the
/// CJK-heuristic fallback for uncompressed pieces that decode as mostly CJK
/// ideographs (a strong signal the piece is actually CP1252, not UTF-16).
///
/// Returns fewer than `char_count` characters -- and records a warning via
/// [`push_piece_overrun_warning`] -- when the piece's declared FC/length
/// A decoded piece: its characters, plus for each character the byte offset
/// (`FC`) in the WordDocument stream it was read from.
///
/// The two vectors are always the same length. Paragraph properties are
/// FC-addressed while text is CP-addressed, so #1550 needs this pairing to
/// bind a paragraph to its `PAPX`; the uncompressed path can drop a code unit
/// that is not a scalar value, which is why the offsets are recorded during
/// decoding rather than recomputed from an index afterwards.
///
/// `fc_ends` holds the offset one *past* each character, because that is what
/// an FKP's `rgfc` entry stores for a paragraph. Recording the end during
/// decoding avoids re-deriving it as `fc + 1`, which is only correct for
/// compressed pieces -- an uncompressed character is two bytes wide. ~keep
#[derive(Default)]
struct DecodedPiece {
    chars: Vec<char>,
    fc_ends: Vec<u32>,
}

impl DecodedPiece {
    fn len(&self) -> usize {
        self.chars.len()
    }

    fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

/// Decode one piece's characters, recording each character's `FC`.
fn decode_piece_chars(
    word_doc: &[u8],
    fc_raw: u32,
    char_count: usize,
    piece_index: usize,
    warnings: &mut Vec<ProcessingWarning>,
) -> DecodedPiece {
    let is_compressed = (fc_raw & 0x4000_0000) != 0;
    let fc = (fc_raw & 0x3FFF_FFFF) as usize;
    // Compressed (CP1252) pieces address `word_doc` at half the raw FC value;
    // uncompressed (UTF-16LE) pieces address it directly. ~keep
    let byte_offset = if is_compressed { fc / 2 } else { fc };

    let decode_cp1252 = |start: usize, end: usize| -> DecodedPiece {
        if start >= end {
            return DecodedPiece::default();
        }
        DecodedPiece {
            chars: word_doc[start..end].iter().map(|&b| cp1252_to_char(b)).collect(),
            fc_ends: (start..end).map(|fc| (fc + 1) as u32).collect(),
        }
    };

    if is_compressed {
        let end = byte_offset + char_count;
        let available_end = if end > word_doc.len() {
            push_piece_overrun_warning(warnings, piece_index, end, word_doc.len());
            word_doc.len()
        } else {
            end
        };
        decode_cp1252(byte_offset, available_end)
    } else {
        let end = byte_offset + char_count * 2;
        let available_end = if end > word_doc.len() {
            push_piece_overrun_warning(warnings, piece_index, end, word_doc.len());
            byte_offset + ((word_doc.len().saturating_sub(byte_offset)) / 2) * 2
        } else {
            end
        };
        let piece: DecodedPiece = if byte_offset >= available_end {
            DecodedPiece::default()
        } else {
            let mut chars = Vec::new();
            let mut fc_ends = Vec::new();
            for (i, unit) in word_doc[byte_offset..available_end].chunks_exact(2).enumerate() {
                if let Some(c) = char::from_u32(u16::from_le_bytes([unit[0], unit[1]]) as u32) {
                    chars.push(c);
                    fc_ends.push((byte_offset + i * 2 + 2) as u32);
                }
            }
            DecodedPiece { chars, fc_ends }
        };

        let suspicious = piece
            .chars
            .iter()
            .filter(|c| (0x4E00..=0x9FFF).contains(&(**c as u32)))
            .count();
        if piece.len() > 4 && suspicious > piece.len() / 4 {
            let cp1252_end = (byte_offset + char_count).min(word_doc.len());
            return decode_cp1252(byte_offset, cp1252_end);
        }
        piece
    }
}

/// Append the overlap between `[cp_start, cp_start + chars.len())` and `range`
/// to `out`, translating the overlap into an index range on `chars`.
fn append_range_overlap(
    piece: &DecodedPiece,
    cp_start: usize,
    range: SubdocRange,
    out: &mut String,
    out_fcs: Option<&mut Vec<u32>>,
) {
    if range.len() == 0 {
        return;
    }
    let piece_end = cp_start + piece.len();
    let overlap_start = cp_start.max(range.start);
    let overlap_end = piece_end.min(range.end);
    if overlap_start < overlap_end {
        let from = overlap_start - cp_start;
        let to = overlap_end - cp_start;
        out.extend(&piece.chars[from..to]);
        if let Some(out_fcs) = out_fcs {
            out_fcs.extend_from_slice(&piece.fc_ends[from..to]);
        }
    }
}

/// Fields of [`extract_text_from_piece_table`] that [`process_piece`] needs but never
/// mutates, bundled purely to keep that function's parameter list manageable.
struct PieceTableContext<'a> {
    /// Total number of declared pieces (`PlcPcd` entries).
    n: usize,
    plc_size: usize,
    plc_pcd: &'a [u8],
    word_doc: &'a [u8],
    total_cp: usize,
    ranges: &'a SubdocRanges,
}

/// Process one entry of the piece table (`PlcPcd`), appending its decoded characters to
/// the matching subdocument range(s) in `text`. Returns `false` when the outer loop over
/// pieces in [`extract_text_from_piece_table`] must stop (a truncated table, or a piece
/// starting past `total_cp`); `true` otherwise, including when this particular piece
/// contributed nothing.
fn process_piece(
    i: usize,
    ctx: &PieceTableContext,
    warnings: &mut Vec<ProcessingWarning>,
    text: &mut SubdocumentText,
) -> bool {
    let cp_start_off = i * 4;
    let cp_end_off = (i + 1) * 4;
    let pcd_off = (ctx.n + 1) * 4 + i * 8;

    if cp_end_off + 4 > ctx.plc_size || pcd_off + 8 > ctx.plc_size {
        let n = ctx.n;
        crate::core::diagnostics::push_warning(
            warnings,
            DOC_WARNING_SOURCE,
            format!(
                "Piece table truncated after {i} of {n} declared pieces; remaining document text was not extracted"
            ),
        );
        return false;
    }

    let plc_pcd = ctx.plc_pcd;
    let cp_start = u32::from_le_bytes([
        plc_pcd[cp_start_off],
        plc_pcd[cp_start_off + 1],
        plc_pcd[cp_start_off + 2],
        plc_pcd[cp_start_off + 3],
    ]) as usize;

    let cp_end = u32::from_le_bytes([
        plc_pcd[cp_end_off],
        plc_pcd[cp_end_off + 1],
        plc_pcd[cp_end_off + 2],
        plc_pcd[cp_end_off + 3],
    ]) as usize;

    if cp_start >= ctx.total_cp {
        return false;
    }

    let fc_raw = u32::from_le_bytes([
        plc_pcd[pcd_off + 2],
        plc_pcd[pcd_off + 3],
        plc_pcd[pcd_off + 4],
        plc_pcd[pcd_off + 5],
    ]);

    let mut char_count = cp_end.saturating_sub(cp_start);
    if cp_start + char_count > ctx.total_cp {
        char_count = ctx.total_cp.saturating_sub(cp_start);
    }
    if char_count == 0 {
        return true;
    }

    let piece = decode_piece_chars(ctx.word_doc, fc_raw, char_count, i, warnings);
    if piece.is_empty() {
        return true;
    }

    let ranges = ctx.ranges;
    append_range_overlap(
        &piece,
        cp_start,
        ranges.main,
        &mut text.main,
        Some(&mut text.main_fc_ends),
    );
    append_range_overlap(&piece, cp_start, ranges.footnote, &mut text.footnote, None);
    append_range_overlap(&piece, cp_start, ranges.header, &mut text.header, None);
    append_range_overlap(&piece, cp_start, ranges.annotation, &mut text.annotation, None);
    append_range_overlap(&piece, cp_start, ranges.textbox, &mut text.textbox, None);
    true
}

/// Extract text from the piece table (PlcPcd), bucketing each piece's
/// characters into the subdocument range they fall in (#77: previously any
/// piece whose CP range started at or after `ccpText` -- i.e. every
/// footnote, header/footer, comment and text-box piece -- was silently
/// skipped).
fn extract_text_from_piece_table(
    word_doc: &[u8],
    plc_pcd: &[u8],
    ranges: &SubdocRanges,
    total_cp: usize,
    warnings: &mut Vec<ProcessingWarning>,
    list_tables: &papx::ListTables,
) -> Result<MainText> {
    let plc_size = plc_pcd.len();
    if plc_size < 16 {
        return Err(XbergError::parsing("PlcPcd too small"));
    }

    let n = (plc_size - 4) / 12;
    if n == 0 {
        return Ok(MainText::text_only(String::new()));
    }

    let mut text = SubdocumentText::default();
    let piece_ctx = PieceTableContext {
        n,
        plc_size,
        plc_pcd,
        word_doc,
        total_cp,
        ranges,
    };

    for i in 0..n {
        if !process_piece(i, &piece_ctx, warnings, &mut text) {
            break;
        }
    }

    if ranges.has_unextracted_subdocument() {
        crate::core::diagnostics::push_warning(
            warnings,
            DOC_WARNING_SOURCE,
            "Document contains endnote and/or header-text-box content that is not extracted",
        );
    }

    let mut content = normalize_doc_text(&text.main);
    for (label, section) in [
        ("Footnotes", &text.footnote),
        ("Headers and Footers", &text.header),
        ("Comments", &text.annotation),
        ("Text Boxes", &text.textbox),
    ] {
        let normalized_section = normalize_doc_text(section);
        if !normalized_section.is_empty() {
            if !content.is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(label);
            content.push_str("\n\n");
            content.push_str(&normalized_section);
        }
    }

    Ok(MainText {
        paragraphs: split_main_paragraphs(&text.main, &text.main_fc_ends, list_tables),
        content,
    })
}
