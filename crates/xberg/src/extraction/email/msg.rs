use super::msg_props::*;
use super::*;

/// Parse .msg file content (Outlook format).
///
/// Reads MSG files directly via the CFB (OLE Compound Document) format,
/// extracting text properties and attachment metadata without the overhead
/// of hex-encoding attachment binary data (which caused hangs on large files
/// with the previous `msg_parser` dependency).
///
/// Some MSG files have FAT headers declaring more sectors than the file
/// actually contains.  The strict `cfb` crate rejects these.  When that
/// happens we pad the data with zero bytes so the sector count matches
/// the FAT and retry – the real streams are still within the original
/// data range and parse correctly.
///
pub(crate) fn parse_msg_content(data: &[u8], fallback_codepage: Option<u32>) -> Result<EmailExtractionResult> {
    parse_msg_content_with_nested(data, fallback_codepage, &SecurityLimits::default()).map(|(result, _, _)| result)
}

/// Parse .msg file content (Outlook format), also returning any embedded
/// (`afEmbeddedMessage`) MSG attachments found while parsing, flattened
/// across nesting depth, plus any warnings raised (e.g. a `.msg`-in-`.msg`
/// chain that hit `security_limits`' nesting-depth cap).
pub(crate) fn parse_msg_content_with_nested(
    data: &[u8],
    fallback_codepage: Option<u32>,
    security_limits: &SecurityLimits,
) -> Result<(
    EmailExtractionResult,
    Vec<EmailExtractionResult>,
    Vec<ProcessingWarning>,
)> {
    use std::borrow::Cow;
    use std::io::Cursor;

    let padded: Cow<'_, [u8]>;
    let data_ref: &[u8] = match cfb::CompoundFile::open(Cursor::new(data)) {
        Ok(_) => data,
        Err(_first_err) => {
            padded = pad_cfb_to_fat_size(data);
            if std::ptr::eq(padded.as_ref(), data) {
                return Err(XbergError::parsing(format!("Failed to parse MSG file: {_first_err}")));
            }
            &padded
        }
    };

    let mut comp = cfb::CompoundFile::open(Cursor::new(data_ref))
        .map_err(|e| XbergError::parsing(format!("Failed to parse MSG file: {e}")))?;

    // Reuse the same nesting-depth counter `SecurityLimits` provides for
    // every other format (XML/HTML/JSON) to guard the `.msg`-in-`.msg`
    // recursion hazard, rather than a bespoke counter local to this parser.
    let mut budget = SecurityBudget::from_limits(security_limits);
    let mut warnings = Vec::new();
    let (result, nested_embedded_messages) =
        extract_msg_from_cfb_at(&mut comp, "", fallback_codepage, &mut budget, &mut warnings)?;

    Ok((result, nested_embedded_messages, warnings))
}

/// Pad an OLE/CFB file so the sector count matches the FAT header.
///
/// Some MSG writers emit FAT tables that reference sectors beyond the
/// physical end of the file.  The `cfb` crate rightfully rejects these
/// as "Malformed FAT".  By zero-padding to the declared size we let cfb
/// open the file; streams within the original range parse normally while
/// the padded area is treated as free sectors.
pub(super) fn pad_cfb_to_fat_size(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    use std::borrow::Cow;

    if data.len() < 76 || data[..4] != [0xD0, 0xCF, 0x11, 0xE0] {
        return Cow::Borrowed(data);
    }

    let sector_power = u16::from_le_bytes([data[30], data[31]]) as u32;
    if !(9..=16).contains(&sector_power) {
        return Cow::Borrowed(data);
    }
    let sector_size = 1u64 << sector_power;

    let fat_sectors = u32::from_le_bytes([data[44], data[45], data[46], data[47]]) as u64;
    let fat_entries = fat_sectors * (sector_size / 4);
    let needed = (1 + fat_entries) * sector_size;

    if needed > 256 * 1024 * 1024 || (data.len() as u64) >= needed {
        return Cow::Borrowed(data);
    }

    let mut padded = data.to_vec();
    padded.resize(needed as usize, 0);
    Cow::Owned(padded)
}

/// Enumerate storage paths that are *direct* children of `message_root` whose
/// name starts with `name_prefix`.
///
/// A message may contain an embedded message attachment (`afEmbeddedMessage`),
/// whose own attachments/recipients live deeper in the tree under
/// `{message_root}/__attach_.../__substg1.0_3701000D/...`. A naive whole-tree
/// `walk()` filtered only by name prefix would incorrectly attribute those
/// deeper entries to the outer message, so entries are filtered to those whose
/// immediate parent path equals `message_root` exactly.
pub(super) fn direct_child_storage_paths<F: std::io::Read + std::io::Seek>(
    comp: &cfb::CompoundFile<F>,
    message_root: &str,
    name_prefix: &str,
) -> Vec<String> {
    let entries: Vec<cfb::Entry> = if message_root.is_empty() {
        comp.walk().collect()
    } else {
        match comp.walk_storage(message_root) {
            Ok(iter) => iter.collect(),
            Err(_) => return Vec::new(),
        }
    };

    entries
        .into_iter()
        .filter(|e| e.is_storage() && e.name().starts_with(name_prefix))
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|path| path.rsplit_once('/').map(|(parent, _)| parent) == Some(message_root))
        .collect()
}

/// Fields [`process_msg_attachment`] needs beyond `comp` and the attachment's own path.
/// Bundled purely to keep that function's parameter list manageable: `budget` and
/// `warnings` thread the nesting-depth cap through an embedded-message attachment's own
/// recursive call into [`extract_msg_from_cfb_at`].
pub(super) struct MsgAttachmentContext<'a> {
    fallback_codepage: Option<u32>,
    codepage: Option<u32>,
    budget: &'a mut SecurityBudget,
    warnings: &'a mut Vec<ProcessingWarning>,
}

/// Handle an `afEmbeddedMessage` attachment: recurse into [`extract_msg_from_cfb_at`] on
/// its nested CFB storage, budget permitting, pushing a `message/rfc822` attachment
/// either way and (only when the recursion actually ran) the parsed nested message onto
/// `nested_embedded_messages`.
pub(super) fn process_embedded_msg_attachment<F: std::io::Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    path: &str,
    filename: Option<String>,
    ctx: &mut MsgAttachmentContext,
    attachments: &mut Vec<EmailAttachment>,
    nested_embedded_messages: &mut Vec<EmailExtractionResult>,
) -> Result<()> {
    let embedded_storage_path = format!("{path}/__substg1.0_{PID_TAG_ATTACH_DATA_OBJECT:04X}000D");

    if ctx.budget.enter().is_ok() {
        let recursed = extract_msg_from_cfb_at(
            comp,
            &embedded_storage_path,
            ctx.fallback_codepage,
            ctx.budget,
            ctx.warnings,
        );
        ctx.budget.leave();
        let (nested_result, deeper_nested) = recursed?;
        let filename = filename
            .or_else(|| nested_result.subject.clone())
            .or_else(|| Some("embedded_message".to_string()));

        attachments.push(EmailAttachment {
            name: filename.clone(),
            filename,
            mime_type: Some("message/rfc822".to_string()),
            size: None,
            is_image: false,
            data: None,
        });

        nested_embedded_messages.push(nested_result);
        nested_embedded_messages.extend(deeper_nested);
    } else {
        // `budget.enter()` still incremented the counter even though it
        // rejected this level; balance it immediately so sibling
        // attachments at the same depth aren't spuriously capped too. ~keep
        ctx.budget.leave();
        ctx.warnings.push(ProcessingWarning {
            source: Cow::Borrowed("msg_embedded_message_extraction"),
            message: Cow::Owned(format!(
                "Stopped extracting embedded message at '{path}': nesting depth cap reached; \
                 the embedded message and anything nested inside it were left unextracted"
            )),
        });
        attachments.push(EmailAttachment {
            name: filename.clone(),
            filename,
            mime_type: Some("message/rfc822".to_string()),
            size: None,
            is_image: false,
            data: None,
        });
    }
    Ok(())
}

/// Process one `__attach_*` storage under a `.msg` message, pushing the resulting
/// [`EmailAttachment`] onto `attachments`. An `afEmbeddedMessage` attachment recurses
/// into [`extract_msg_from_cfb_at`] (budget permitting) and additionally appends its own
/// (and any deeper) nested message to `nested_embedded_messages`.
pub(super) fn process_msg_attachment<F: std::io::Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    path: &str,
    ctx: &mut MsgAttachmentContext,
    attachments: &mut Vec<EmailAttachment>,
    nested_embedded_messages: &mut Vec<EmailExtractionResult>,
) -> Result<()> {
    let long_name = read_msg_string_prop(comp, path, 0x3707, ctx.codepage);
    let short_name = read_msg_string_prop(comp, path, 0x3704, ctx.codepage);
    let display_name = read_msg_string_prop(comp, path, 0x3001, ctx.codepage);
    let extension = read_msg_string_prop(comp, path, 0x3703, ctx.codepage);
    let mime_tag = read_msg_string_prop(comp, path, 0x370E, ctx.codepage);

    let filename = long_name
        .or(short_name)
        .or_else(|| display_name.clone())
        .or_else(|| extension.map(|ext| format!("attachment{ext}")));

    let attach_method = read_msg_attach_or_recip_int_prop(comp, path, PID_TAG_ATTACH_METHOD);
    let embedded_storage_path = format!("{path}/__substg1.0_{PID_TAG_ATTACH_DATA_OBJECT:04X}000D");
    let is_embedded_message =
        attach_method == Some(ATTACH_METHOD_EMBEDDED_MSG) && comp.is_storage(&embedded_storage_path);

    if is_embedded_message {
        return process_embedded_msg_attachment(comp, path, filename, ctx, attachments, nested_embedded_messages);
    }

    let bin_path = format!("{path}/__substg1.0_37010102");
    let binary_data = read_msg_stream(comp, &bin_path);
    let size = binary_data.as_ref().map(Vec::len);
    let att_data = binary_data.map(Bytes::from);

    let mime_type = mime_tag
        .filter(|s| !s.is_empty())
        .or_else(|| Some("application/octet-stream".to_string()));
    let is_image = mime_type.as_ref().map(|m| is_image_mime_type(m)).unwrap_or(false);

    attachments.push(EmailAttachment {
        name: filename.clone(),
        filename,
        mime_type,
        size,
        is_image,
        data: att_data,
    });
    Ok(())
}

/// Fields [`build_msg_metadata`] turns into the `.msg` message's metadata map. Bundled
/// purely to keep that function's parameter list manageable.
pub(super) struct MsgMetadataFields<'a> {
    subject: &'a Option<String>,
    from_email: &'a Option<String>,
    sender_name: &'a Option<String>,
    to_emails: &'a [String],
    cc_emails: &'a [String],
    bcc_emails: &'a [String],
    date: &'a Option<String>,
    message_id: &'a Option<String>,
}

pub(super) fn build_msg_metadata(fields: &MsgMetadataFields) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    if let Some(subj) = fields.subject {
        metadata.insert("subject".to_string(), subj.to_string());
    }
    if let Some(from) = fields.from_email {
        metadata.insert("email_from".to_string(), from.to_string());
    }
    if let Some(name) = fields.sender_name
        && !name.is_empty()
    {
        metadata.insert("from_name".to_string(), name.to_string());
    }
    if !fields.to_emails.is_empty() {
        metadata.insert("email_to".to_string(), fields.to_emails.join(", "));
    }
    if !fields.cc_emails.is_empty() {
        metadata.insert("email_cc".to_string(), fields.cc_emails.join(", "));
    }
    if !fields.bcc_emails.is_empty() {
        metadata.insert("email_bcc".to_string(), fields.bcc_emails.join(", "));
    }
    if let Some(dt) = fields.date {
        metadata.insert("date".to_string(), dt.to_string());
    }
    if let Some(msg_id) = fields.message_id {
        metadata.insert("message_id".to_string(), msg_id.to_string());
    }
    metadata
}

/// The message-level (non-attachment) fields [`extract_msg_from_cfb_at`] reads before
/// walking attachments, plus the resolved `codepage` its attachment loop also needs.
/// Bundled purely to keep that function's body shorter.
pub(super) struct MsgCoreFields {
    codepage: Option<u32>,
    subject: Option<String>,
    sender_name: Option<String>,
    from_email: Option<String>,
    plain_text: Option<String>,
    html_content: Option<String>,
    message_id: Option<String>,
    date: Option<String>,
    to_emails: Vec<String>,
    cc_emails: Vec<String>,
    bcc_emails: Vec<String>,
    content: String,
}

/// Read every message-level (non-attachment) field of a `.msg` message or embedded
/// message rooted at `message_root`.
pub(super) fn read_msg_core_fields<F: std::io::Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    message_root: &str,
    fallback_codepage: Option<u32>,
) -> MsgCoreFields {
    let message_codepage = read_msg_int_prop(comp, message_root, PID_TAG_MESSAGE_CODEPAGE);
    let internet_codepage = read_msg_int_prop(comp, message_root, PID_TAG_INTERNET_CODEPAGE);
    let codepage = message_codepage.or(internet_codepage).or(fallback_codepage);
    let html_codepage = internet_codepage.or(message_codepage).or(fallback_codepage);

    let subject = read_msg_string_prop(comp, message_root, 0x0037, codepage);
    let sender_name = read_msg_string_prop(comp, message_root, 0x0C1A, codepage);
    let from_email = read_msg_string_prop(comp, message_root, 0x0C1F, codepage)
        .or_else(|| read_msg_string_prop(comp, message_root, 0x0065, codepage))
        .filter(|s| !s.is_empty());
    let body = read_msg_string_prop(comp, message_root, 0x1000, codepage);
    let html_body = read_msg_html_prop(comp, message_root, html_codepage);
    let message_id = read_msg_string_prop(comp, message_root, 0x1035, codepage).filter(|s| !s.is_empty());

    let date = read_msg_filetime_prop(comp, message_root, 0x0039)
        .or_else(|| read_msg_filetime_prop(comp, message_root, 0x0E06))
        .or_else(|| {
            let headers = read_msg_string_prop(comp, message_root, 0x007D, codepage);
            headers.as_ref().and_then(|h| {
                h.lines()
                    .find(|line| line.starts_with("Date:"))
                    .map(|line| line.trim_start_matches("Date:").trim().to_string())
            })
        });

    let (to_emails, cc_emails, bcc_emails) = read_msg_recipients(comp, message_root, codepage);

    let rtf_body = read_msg_stream(comp, &format!("{message_root}/__substg1.0_10090102"))
        .and_then(|data| decompress_rtf_compressed(&data))
        .map(|rtf| strip_rtf_to_plain_text(&rtf))
        .filter(|s| !s.is_empty());

    let plain_text = body.filter(|s| !s.is_empty());
    let html_content = html_body.filter(|s| !s.is_empty());

    let content = if let Some(ref plain) = plain_text {
        plain.clone()
    } else if let Some(ref html) = html_content {
        clean_html_content(html)
    } else if let Some(ref rtf) = rtf_body {
        rtf.clone()
    } else {
        String::new()
    };

    MsgCoreFields {
        codepage,
        subject,
        sender_name,
        from_email,
        plain_text,
        html_content,
        message_id,
        date,
        to_emails,
        cc_emails,
        bcc_emails,
        content,
    }
}

/// Internal: extract email fields from an already-opened CFB compound file.
///
/// `message_root` is `""` for the top-level MSG message, or the CFB storage
/// path of an embedded message (`afEmbeddedMessage` attachment) when called
/// recursively. `budget` guards against pathological/malicious
/// message-in-message nesting via its `SecurityLimits`-derived
/// [`crate::extractors::security::DepthValidator`] — the same nesting-depth
/// counter every other format uses, rather than a bespoke one for MSG.
/// `warnings` collects a [`ProcessingWarning`] whenever the depth cap stops a
/// nested embedded message from being extracted further.
///
/// Returns the parsed message plus a flat list of any embedded MSG messages
/// discovered at or below this level.
pub(super) fn extract_msg_from_cfb_at<F: std::io::Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    message_root: &str,
    fallback_codepage: Option<u32>,
    budget: &mut SecurityBudget,
    warnings: &mut Vec<ProcessingWarning>,
) -> Result<(EmailExtractionResult, Vec<EmailExtractionResult>)> {
    let core = read_msg_core_fields(comp, message_root, fallback_codepage);

    let attach_paths = direct_child_storage_paths(comp, message_root, "__attach_");
    let mut attachments = Vec::with_capacity(attach_paths.len());
    let mut nested_embedded_messages = Vec::new();
    let mut attach_ctx = MsgAttachmentContext {
        fallback_codepage,
        codepage: core.codepage,
        budget,
        warnings,
    };
    for path in &attach_paths {
        process_msg_attachment(
            comp,
            path,
            &mut attach_ctx,
            &mut attachments,
            &mut nested_embedded_messages,
        )?;
    }

    let metadata = build_msg_metadata(&MsgMetadataFields {
        subject: &core.subject,
        from_email: &core.from_email,
        sender_name: &core.sender_name,
        to_emails: &core.to_emails,
        cc_emails: &core.cc_emails,
        bcc_emails: &core.bcc_emails,
        date: &core.date,
        message_id: &core.message_id,
    });

    Ok((
        EmailExtractionResult {
            subject: core.subject,
            from_email: core.from_email,
            to_emails: core.to_emails,
            cc_emails: core.cc_emails,
            bcc_emails: core.bcc_emails,
            date: core.date,
            message_id: core.message_id,
            plain_text: core.plain_text,
            html_content: core.html_content,
            content: core.content,
            attachments,
            metadata,
        },
        nested_embedded_messages,
    ))
}
