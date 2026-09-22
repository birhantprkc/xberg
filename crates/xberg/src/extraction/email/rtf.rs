/// Maximum capacity pre-allocated for a decompressed RTF body (16 MiB).
///
/// `raw_size` in the OXRTFCP header is attacker-controlled; using it directly
/// as a `Vec::with_capacity` argument lets a crafted `.msg` file trigger a 4 GiB
/// allocation.  This constant caps the hint — the `Vec` grows freely beyond it if
/// the actual decompressed content is larger, so correctness is unaffected.
const MAX_RTF_DECOMPRESSED_CAPACITY: usize = 16 * 1024 * 1024;

/// Pre-filled dictionary used by the MS-OXRTFCP compressed RTF format.
///
/// See [MS-OXRTFCP] section 2.1.3.1.
const COMPRESSED_RTF_PREBUF: &[u8] = b"{\\rtf1\\ansi\\mac\\deff0\\deftab720{\\fonttbl;}\
{\\f0\\fnil \\froman \\fswiss \\fmodern \\fscript \\fdecor MS Sans SerifSymbolArialTimes New Roman\
Courier{\\colortbl\\red0\\green0\\blue0\r\n\\par \\pard\\plain\\f0\\fs20\\b\\i\\ul\\ob\\strike\
\\scaps\\outline\\shadow\\imprint\\emboss\\lang1024\\sbasedon1033\\fcharset0 {\\*\\cs10 \\additive \
Default Paragraph Font}";

/// Decompress a PR_RTF_COMPRESSED stream per the MS-OXRTFCP specification.
///
/// Returns `None` when the data is too short, has a bad magic number, or
/// the decompression runs past declared bounds.
pub(crate) fn decompress_rtf_compressed(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }

    let comp_size = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let raw_size = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let magic = u32::from_le_bytes(data[8..12].try_into().ok()?);

    if magic == 0x414c_454d {
        return Some(data.get(16..16 + comp_size.saturating_sub(12))?.to_vec());
    }
    if magic != 0x75465a4c {
        return None;
    }

    let mut dict = [0u8; 4096];
    let prebuf_len = COMPRESSED_RTF_PREBUF.len();
    dict[..prebuf_len].copy_from_slice(COMPRESSED_RTF_PREBUF);
    let mut dict_write = prebuf_len;

    let input = data.get(16..)?;
    let end = (comp_size.saturating_sub(12)).min(input.len());

    let mut output = Vec::with_capacity((raw_size as usize).min(MAX_RTF_DECOMPRESSED_CAPACITY));
    let mut pos = 0usize;

    while pos < end {
        let control = *input.get(pos)?;
        pos += 1;

        for bit in (0..8).rev() {
            if pos >= end {
                return Some(output);
            }

            if control & (1 << bit) != 0 {
                let hi = *input.get(pos)? as u16;
                let lo = *input.get(pos + 1)? as u16;
                pos += 2;

                let offset = ((hi << 4) | (lo >> 4)) as usize;
                let length = (lo & 0x0F) as usize + 2;

                for i in 0..length {
                    let byte = dict[(offset + i) & 0xFFF];
                    output.push(byte);
                    dict[dict_write & 0xFFF] = byte;
                    dict_write += 1;
                }
            } else {
                let byte = *input.get(pos)?;
                pos += 1;
                output.push(byte);
                dict[dict_write & 0xFFF] = byte;
                dict_write += 1;
            }
        }
    }

    Some(output)
}

/// Handle a `{` group open at `bytes[i]`, updating `depth` and, when the group is a
/// destination this converter never emits text for (`\*` marked-content groups, and the
/// `\fonttbl`/`\colortbl`/`\stylesheet`/`\info` destinations), setting `skip_depth` to the
/// depth being skipped from. Returns the index just past the `{`.
fn handle_rtf_group_open(
    text: &str,
    bytes: &[u8],
    i: usize,
    depth: &mut usize,
    skip_depth: &mut Option<usize>,
) -> usize {
    *depth += 1;
    let i = i + 1;
    if i + 1 < bytes.len() && bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'*') && skip_depth.is_none() {
        *skip_depth = Some(*depth);
    }
    if skip_depth.is_none() {
        let rest = &text[i..];
        if rest.starts_with("\\fonttbl")
            || rest.starts_with("\\colortbl")
            || rest.starts_with("\\stylesheet")
            || rest.starts_with("\\info")
        {
            *skip_depth = Some(*depth);
        }
    }
    i
}

/// Handle a `}` group close at `bytes[i]`, clearing `skip_depth` once its group has
/// closed. Returns the index just past the `}`.
fn handle_rtf_group_close(i: usize, depth: &mut usize, skip_depth: &mut Option<usize>) -> usize {
    if let Some(sd) = *skip_depth
        && *depth <= sd
    {
        *skip_depth = None;
    }
    *depth = depth.saturating_sub(1);
    i + 1
}

/// Handle a `\uNNNN` Unicode escape starting at `bytes[i] == b'u'`, appending the decoded
/// character to `output`. Per the RTF spec, a `\u` escape is followed by one
/// ANSI-fallback character that a reader without Unicode support would show instead;
/// that fallback is consumed here too since it carries no additional text.
fn handle_rtf_unicode_escape(bytes: &[u8], i: usize, len: usize, output: &mut String) -> usize {
    let mut i = i + 1;
    let start = i;
    if i < len && bytes[i] == b'-' {
        i += 1;
    }
    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if let Ok(num_str) = std::str::from_utf8(&bytes[start..i])
        && let Ok(code) = num_str.parse::<i32>()
    {
        let cp = if code < 0 { (code + 65536) as u32 } else { code as u32 };
        if let Some(ch) = char::from_u32(cp) {
            output.push(ch);
        }
    }
    if i < len && bytes[i] == b' ' {
        i += 1;
    }
    if i < len && bytes[i] != b'\\' && bytes[i] != b'{' && bytes[i] != b'}' {
        i += 1;
    }
    i
}

/// Handle a `\wordname[-digits][ ]` control word starting at `bytes[i]`, translating the
/// handful this converter cares about (`par`, `line`, `tab`) to their plain-text
/// equivalents and discarding the rest (and any numeric parameter). Returns the index
/// just past the word, its optional signed numeric parameter, and one trailing space.
fn handle_rtf_control_word_name(text: &str, bytes: &[u8], i: usize, len: usize, output: &mut String) -> usize {
    let word_start = i;
    let mut i = i;
    while i < len && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    let word = &text[word_start..i];

    if i < len && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
        if bytes[i] == b'-' {
            i += 1;
        }
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }

    if i < len && bytes[i] == b' ' {
        i += 1;
    }

    match word {
        "par" | "line" => output.push('\n'),
        "tab" => output.push('\t'),
        _ => {}
    }
    i
}

/// Handle one `\<...>` RTF control word or symbol starting at `bytes[i] == b'\\'`,
/// appending any resulting plain text to `output`. Returns the index just past the
/// control sequence, or `len` when the backslash was the stream's last byte.
fn handle_rtf_control_word(text: &str, bytes: &[u8], i: usize, len: usize, output: &mut String) -> usize {
    let mut i = i + 1;
    if i >= len {
        return len;
    }
    match bytes[i] {
        b'\\' => {
            output.push('\\');
            i += 1;
        }
        b'{' => {
            output.push('{');
            i += 1;
        }
        b'}' => {
            output.push('}');
            i += 1;
        }
        b'\'' => {
            i += 1;
            if i + 2 <= len {
                if let Ok(hex_str) = std::str::from_utf8(&bytes[i..i + 2])
                    && let Ok(byte_val) = u8::from_str_radix(hex_str, 16)
                {
                    let byte_arr = [byte_val];
                    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&byte_arr);
                    output.push_str(&decoded);
                }
                i += 2;
            }
        }
        b'u' if i + 1 < len && (bytes[i + 1].is_ascii_digit() || bytes[i + 1] == b'-') => {
            i = handle_rtf_unicode_escape(bytes, i, len, output);
        }
        _ => {
            i = handle_rtf_control_word_name(text, bytes, i, len, output);
        }
    }
    i
}

/// Collapse RTF's over-eager paragraph/line-break output down to at most two consecutive
/// newlines (one blank line).
fn collapse_rtf_newlines(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut prev_newline_count = 0u32;
    for ch in input.chars() {
        if ch == '\n' {
            prev_newline_count += 1;
            if prev_newline_count <= 2 {
                result.push('\n');
            }
        } else {
            prev_newline_count = 0;
            result.push(ch);
        }
    }
    result
}

/// Strip RTF control sequences and extract plain text.
///
/// Handles `\par` → newline, `\uN` unicode escapes, `{` `}` grouping,
/// and discards other `\command` sequences.  This is intentionally
/// simplified — it covers the typical content produced by Outlook.
pub(crate) fn strip_rtf_to_plain_text(rtf: &[u8]) -> String {
    let text = String::from_utf8_lossy(rtf);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut output = String::with_capacity(len / 2);
    let mut i = 0;
    let mut skip_depth: Option<usize> = None;
    let mut depth: usize = 0;

    while i < len {
        match bytes[i] {
            b'{' => {
                i = handle_rtf_group_open(&text, bytes, i, &mut depth, &mut skip_depth);
            }
            b'}' => {
                i = handle_rtf_group_close(i, &mut depth, &mut skip_depth);
            }
            b'\\' if skip_depth.is_none() => {
                i = handle_rtf_control_word(&text, bytes, i, len, &mut output);
            }
            b'\r' | b'\n' if skip_depth.is_none() => {
                i += 1;
            }
            _ if skip_depth.is_some() => {
                i += 1;
            }
            _ => {
                output.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    collapse_rtf_newlines(&output).trim().to_string()
}
