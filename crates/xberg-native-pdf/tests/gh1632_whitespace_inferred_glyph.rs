//! GH#1632: a glyph was dropped when its *inferred* Unicode was whitespace.
//!
//! `render_cid_direct` decided whether to paint by asking what character the
//! code represented, not whether there was an outline to draw. For a simple
//! TrueType font with a byte-indexed cmap and no `/Encoding`, that character
//! is the GID reverse-mapped through the font's own (1, 0) subtable and read
//! as ASCII or Mac Roman — the subset's private glyph ordering, decoded as an
//! encoding it never expressed. When that decode landed on whitespace the
//! outline was silently discarded and only the advance applied.
//!
//! Six byte values were affected: 0x09-0x0D (TAB LF VT FF CR), 0x20 (SPACE)
//! and 0xCA (U+00A0 via MAC_ROMAN_HIGH). Re-indexed subsets numbering their
//! glyphs from 0x01 upward are ordinary producer output, so such a font walked
//! straight through the whole run.

use xberg_native_pdf::PdfDocument;
use xberg_native_pdf::rendering::{PageRenderer, RenderOptions};

/// A single byte as PDF string content, escaping what the syntax requires.
fn escape(code: u8) -> Vec<u8> {
    match code {
        b'(' | b')' | b'\\' => vec![b'\\', code],
        _ => vec![code],
    }
}

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// A minimal TrueType font: glyph 0 empty, glyph 1 a box, and a single
/// byte-indexed cmap subtable (platform 1, encoding 0, format 0) that maps
/// only 'A' (0x41) to glyph 1. Every other byte resolves to glyph 0.
fn subset_font_mapping(code: u8) -> Vec<u8> {
    let mut glyf: Vec<u8> = Vec::new();
    glyf.extend(be16(1));
    glyf.extend(be16(50));
    glyf.extend(be16(0));
    glyf.extend(be16(450));
    glyf.extend(be16(700));
    glyf.extend(be16(3));
    glyf.extend(be16(0));
    glyf.extend([0x01, 0x01, 0x01, 0x01]);
    for dx in [50i16, 400, 0, -400] {
        glyf.extend(dx.to_be_bytes());
    }
    for dy in [0i16, 0, 700, 0] {
        glyf.extend(dy.to_be_bytes());
    }

    // loca (short format, offset/2): glyph 0 empty, glyph 1 = all of glyf. ~keep
    let mut loca: Vec<u8> = Vec::new();
    loca.extend(be16(0));
    loca.extend(be16(0));
    loca.extend(be16((glyf.len() / 2) as u16));

    let mut head: Vec<u8> = Vec::new();
    head.extend(be32(0x0001_0000));
    head.extend(be32(0));
    head.extend(be32(0));
    head.extend(be32(0x5F0F_3CF5));
    head.extend(be16(0));
    head.extend(be16(1000));
    head.extend([0u8; 16]);
    head.extend(be16(0));
    head.extend(be16(0));
    head.extend(be16(450));
    head.extend(be16(700));
    head.extend(be16(0));
    head.extend(be16(8));
    head.extend(be16(2));
    head.extend(be16(0));
    head.extend(be16(0));

    let mut hhea: Vec<u8> = Vec::new();
    hhea.extend(be32(0x0001_0000));
    hhea.extend(be16(800));
    hhea.extend((-200i16).to_be_bytes());
    hhea.extend(be16(0));
    hhea.extend(be16(500));
    hhea.extend(be16(0));
    hhea.extend(be16(0));
    hhea.extend(be16(450));
    hhea.extend(be16(1));
    hhea.extend(be16(0));
    hhea.extend(be16(0));
    hhea.extend([0u8; 8]);
    hhea.extend(be16(0));
    hhea.extend(be16(2));

    let mut hmtx: Vec<u8> = Vec::new();
    for _ in 0..2 {
        hmtx.extend(be16(500));
        hmtx.extend(be16(0));
    }

    let mut maxp: Vec<u8> = Vec::new();
    maxp.extend(be32(0x0001_0000));
    maxp.extend(be16(2));
    maxp.extend([0u8; 26]);

    let mut cmap: Vec<u8> = Vec::new();
    cmap.extend(be16(0));
    cmap.extend(be16(1));
    cmap.extend(be16(1));
    cmap.extend(be16(0));
    cmap.extend(be32(12));
    cmap.extend(be16(0));
    cmap.extend(be16(262));
    cmap.extend(be16(0));
    let mut glyph_ids = [0u8; 256];
    glyph_ids[usize::from(code)] = 1;
    cmap.extend(glyph_ids);

    let tables: [(&[u8; 4], &Vec<u8>); 7] = [
        (b"cmap", &cmap),
        (b"glyf", &glyf),
        (b"head", &head),
        (b"hhea", &hhea),
        (b"hmtx", &hmtx),
        (b"loca", &loca),
        (b"maxp", &maxp),
    ];
    let num_tables = tables.len() as u16;
    let mut font: Vec<u8> = Vec::new();
    font.extend(be32(0x0001_0000));
    font.extend(be16(num_tables));
    let entry_selector = 15 - num_tables.leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    font.extend(be16(search_range));
    font.extend(be16(entry_selector));
    font.extend(be16(num_tables * 16 - search_range));

    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        font.extend_from_slice(*tag);
        font.extend(be32(0));
        font.extend(be32(offset as u32));
        font.extend(be32(data.len() as u32));
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        font.extend_from_slice(data);
        font.extend(std::iter::repeat_n(0u8, data.len().div_ceil(4) * 4 - data.len()));
    }
    font
}

/// A PDF with `page_count` identical pages, each painting "AB" in the
/// broken embedded font. 'A' paints; 'B' drops.
fn pdf_drawing(code: u8) -> Vec<u8> {
    let page_count = 1usize;
    let font = subset_font_mapping(code);
    let content = [
        b"BT /F1 24 Tf 50 100 Td (".as_slice(),
        &escape(code),
        b") Tj ET".as_slice(),
    ]
    .concat();
    let n_objs = 5 + 2 * page_count;

    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; n_objs + 1];
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    let obj = |buf: &mut Vec<u8>, off: &mut Vec<usize>, id: usize, body: String| {
        off[id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    let kids: Vec<String> = (0..page_count).map(|p| format!("{} 0 R", 6 + 2 * p)).collect();
    obj(&mut buf, &mut off, 1, "<< /Type /Catalog /Pages 2 0 R >>".to_string());
    obj(
        &mut buf,
        &mut off,
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {page_count} >>", kids.join(" ")),
    );
    obj(
        &mut buf,
        &mut off,
        3,
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /BrokenSubset /FirstChar 0 /LastChar 255 \
             /Widths [{}] /FontDescriptor 4 0 R >>",
            vec!["500"; 256].join(" ")
        ),
    );
    obj(
        &mut buf,
        &mut off,
        4,
        "<< /Type /FontDescriptor /FontName /BrokenSubset /Flags 4 /FontBBox [0 0 450 700] \
         /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 5 0 R >>"
            .to_string(),
    );
    off[5] = buf.len();
    buf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} /Length1 {} >>\nstream\n",
            font.len(),
            font.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&font);
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    for p in 0..page_count {
        let page_id = 6 + 2 * p;
        let contents_id = page_id + 1;
        obj(
            &mut buf,
            &mut off,
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {contents_id} 0 R >>"
            ),
        );
        off[contents_id] = buf.len();
        buf.extend_from_slice(format!("{contents_id} 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
        buf.extend_from_slice(&content);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    let xref = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", n_objs + 1).as_bytes());
    for &offset in &off[1..=n_objs] {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            n_objs + 1
        )
        .as_bytes(),
    );
    buf
}

/// Count pixels that received ink.
fn painted_pixels(code: u8) -> usize {
    let doc = PdfDocument::from_bytes(pdf_drawing(code)).expect("parse fixture");
    let mut renderer = PageRenderer::new(RenderOptions::default());
    let image = renderer.render_page(&doc, 0).expect("render page");
    let decoded = image::load_from_memory(&image.data).expect("decode render").to_luma8();
    decoded.pixels().filter(|pixel| pixel.0[0] < 250).count()
}

#[test]
fn a_glyph_whose_inferred_unicode_is_whitespace_is_still_painted() {
    // 0x41 ('A') is the control: its inferred Unicode is not whitespace, so it
    // painted correctly even before the fix. ~keep
    let control = painted_pixels(0x41);
    assert!(control > 0, "control glyph at 0x41 painted nothing ({control} px)");

    // Every byte whose ASCII / Mac Roman decode is whitespace. Each maps to the
    // same box outline as the control, so each must paint the same ink. ~keep
    for code in [0x09u8, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0xCA] {
        let painted = painted_pixels(code);
        assert_eq!(
            painted, control,
            "byte {code:#04x} painted {painted} px but the identical outline at 0x41 painted {control} px \
             — the glyph was suppressed because its inferred Unicode is whitespace"
        );
    }
}
