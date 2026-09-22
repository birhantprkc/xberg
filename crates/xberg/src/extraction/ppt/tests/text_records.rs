use super::super::*;

#[test]
fn test_clean_ppt_text() {
    assert_eq!(clean_ppt_text("Hello\rWorld"), "Hello\nWorld");
    assert_eq!(clean_ppt_text("A\x0BB"), "A\nB");
}

#[test]
fn test_cp1252_to_char() {
    assert_eq!(cp1252_to_char(b'A'), 'A');
    assert_eq!(cp1252_to_char(0x80), '\u{20AC}');
}

#[test]
fn test_extract_ppt_real_file() {
    let test_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_documents/ppt/simple.ppt");
    if !test_file.exists() {
        return;
    }
    let content = std::fs::read(&test_file).expect("Failed to read test PPT");
    let result = extract_ppt_text(&content).expect("Failed to extract PPT text");
    assert!(!result.text.is_empty(), "PPT extraction should produce text");
}

#[test]
fn test_extract_ppt_invalid_data() {
    let result = extract_ppt_text(b"not a ppt file");
    assert!(result.is_err());
}

/// #87 regression: `test_documents/ppt/simple.ppt` has exactly two
/// top-level `Slide` (0x03EE) containers and three `SlideListWithText`
/// (0x0FF0) containers holding only outline-view `SlidePersistAtom`
/// entries. Segmenting on `SlideListWithText` collapsed all real slide
/// text into a single trailing blob and reported the wrong slide count.
#[test]
fn test_extract_ppt_real_file_reports_two_slides() {
    let test_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_documents/ppt/simple.ppt");
    if !test_file.exists() {
        return;
    }
    let content = std::fs::read(&test_file).expect("Failed to read test PPT");
    let result = extract_ppt_text(&content).expect("Failed to extract PPT text");
    assert_eq!(result.slide_count, 2, "simple.ppt has exactly two Slide containers");
}

/// Build one PowerPoint record header (8 bytes: recVerInstance, recType, recLen).
pub(super) fn record_header(rec_ver_instance: u16, rec_type: u16, rec_len: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    buf.extend_from_slice(&rec_ver_instance.to_le_bytes());
    buf.extend_from_slice(&rec_type.to_le_bytes());
    buf.extend_from_slice(&rec_len.to_le_bytes());
    buf
}

/// Build a container record (recVer nibble = 0xF) wrapping `children`.
pub(super) fn container(rec_type: u16, children: &[u8]) -> Vec<u8> {
    let mut buf = record_header(0x000F, rec_type, children.len() as u32);
    buf.extend_from_slice(children);
    buf
}

/// Build a `TextCharsAtom` (UTF-16LE) record for `text`.
pub(super) fn text_chars_atom(text: &str) -> Vec<u8> {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut buf = record_header(0x0000, RT_TEXT_CHARS_ATOM, utf16.len() as u32);
    buf.extend_from_slice(&utf16);
    buf
}

/// Build a `TextHeaderAtom` (MS-PPT 2.13.33) declaring the `TextTypeEnum` value the
/// text atom that follows is typed as.
fn text_header_atom(text_type: u32) -> Vec<u8> {
    let mut buf = record_header(0x0000, RT_TEXT_HEADER_ATOM, 4);
    buf.extend_from_slice(&text_type.to_le_bytes());
    buf
}

/// Build a `SlidePersistAtom` naming `persist_id`. Only the leading `persistIdRef`
/// matters to the reader; the remaining 16 bytes are the documented tail.
fn slide_persist_atom(persist_id: u32) -> Vec<u8> {
    slide_persist_atom_with_slide_id(persist_id, 0)
}

/// Build a `SlidePersistAtom` naming both `persist_id` (which resolves the `Slide`
/// container's stream offset) and `slide_id` (what `NotesAtom.slideIdRef` names the
/// slide by -- #1640). Layout per MS-PPT 2.4.14: `persistIdRef` (4), `flags` (4),
/// `numberTexts` (4), `slideId` (4), `reserved2` (4).
fn slide_persist_atom_with_slide_id(persist_id: u32, slide_id: u32) -> Vec<u8> {
    let mut buf = record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 20);
    buf.extend_from_slice(&persist_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // flags
    buf.extend_from_slice(&[0u8; 4]); // numberTexts
    buf.extend_from_slice(&slide_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // reserved2
    buf
}

/// Build a `NotesAtom` (MS-PPT 2.5.7): `slideIdRef` (4 bytes), then `slideFlags` (2)
/// and `unused` (2) -- neither read here.
fn notes_atom(slide_id_ref: u32) -> Vec<u8> {
    let mut buf = record_header(0x0000, RT_NOTES_ATOM, 8);
    buf.extend_from_slice(&slide_id_ref.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]);
    buf
}

/// Build a `PersistDirectoryAtom` holding one single-id entry per `(id, offset)` pair.
fn persist_directory(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, offset) in entries {
        body.extend_from_slice(&((1u32 << PERSIST_COUNT_SHIFT) | (id & PERSIST_ID_MASK)).to_le_bytes());
        body.extend_from_slice(&offset.to_le_bytes());
    }
    let mut buf = record_header(0x0000, RT_PERSIST_DIRECTORY_ATOM, body.len() as u32);
    buf.extend_from_slice(&body);
    buf
}

/// Build a `UserEditAtom`. `offset_last_edit` is 0 for the first save. `doc_persist_id_ref`
/// is the persist id of this save's live `DocumentContainer` (#1639).
fn user_edit_atom(offset_last_edit: u32, offset_persist_directory: u32, doc_persist_id_ref: u32) -> Vec<u8> {
    let mut buf = record_header(0x0000, RT_USER_EDIT_ATOM, 28);
    buf.extend_from_slice(&1u32.to_le_bytes()); // lastSlideIdRef
    buf.extend_from_slice(&[0u8; 4]); // version / minorVersion / majorVersion
    buf.extend_from_slice(&offset_last_edit.to_le_bytes());
    buf.extend_from_slice(&offset_persist_directory.to_le_bytes());
    buf.extend_from_slice(&doc_persist_id_ref.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // persistIdSeed, lastView, unused
    buf
}

/// Build a `Current User` stream whose `CurrentUserAtom` points at `offset_to_current_edit`.
fn current_user_stream(offset_to_current_edit: u32) -> Vec<u8> {
    let mut buf = record_header(0x0000, 0x0FF6, 0x14);
    buf.extend_from_slice(&0x14u32.to_le_bytes()); // size
    buf.extend_from_slice(&0xE391_C05Fu32.to_le_bytes()); // headerToken
    buf.extend_from_slice(&offset_to_current_edit.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf
}

/// GH#1614: a `.ppt` stream is append-only across saves, so a superseded copy of a
/// slide is still present in the bytes. Walking the stream counts it as a slide of the
/// presentation; the live persist directory does not name it.
#[test]
fn a_superseded_slide_revision_is_not_extracted_as_a_slide() {
    let stale = container(RT_SLIDE, &text_chars_atom("stale revision"));
    let live_one = container(RT_SLIDE, &text_chars_atom("first slide"));
    let live_two = container(RT_SLIDE, &text_chars_atom("second slide"));

    let mut data = Vec::new();
    let stale_offset = data.len() as u32;
    data.extend_from_slice(&stale);
    let one_offset = data.len() as u32;
    data.extend_from_slice(&live_one);
    let two_offset = data.len() as u32;
    data.extend_from_slice(&live_two);

    let slide_list = container(
        RT_SLIDE_LIST_WITH_TEXT,
        &[slide_persist_atom(1), slide_persist_atom(2)].concat(),
    );
    let document = container(RT_DOCUMENT, &slide_list);
    let document_offset = data.len() as u32;
    data.extend_from_slice(&document);

    // Older save: persist id 1 pointed at the stale copy. Newer save: it points at the
    // live one, and adds id 2. Persist id 20 is the `DocumentContainer` itself, named by
    // both saves' `UserEditAtom.docPersistIdRef` (#1639).
    let old_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[(1, stale_offset)]));
    let new_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[
        (1, one_offset),
        (2, two_offset),
        (20, document_offset),
    ]));

    let old_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(0, old_dir_offset, 20));
    let new_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(old_edit_offset, new_dir_offset, 20));

    let live =
        live_slide_offsets(&data, &current_user_stream(new_edit_offset)).expect("the persist chain must resolve");
    assert_eq!(
        live.offsets,
        vec![one_offset as usize, two_offset as usize],
        "the stale revision's offset must not appear in the live slide list"
    );

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, Some(&live), &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 2, "three Slide containers, two live slides");
    assert_eq!(slides[0].text, "first slide");
    assert_eq!(slides[1].text, "second slide");
    assert!(
        !slides.iter().any(|slide| slide.text.contains("stale")),
        "a superseded revision must not reach the output"
    );

    // The fixture reproduces the defect, rather than merely being consistent with the
    // fix: walked without the live list -- which is what this extractor did for every
    // deck -- the stale revision is extracted and numbered as slide 1. ~keep
    let mut stream_order_warnings = Vec::new();
    let (stream_order_slides, _, _) = extract_texts_from_records(&data, false, None, &mut stream_order_warnings)
        .expect("record parsing should succeed");
    assert_eq!(
        stream_order_slides.len(),
        3,
        "stream order counts the superseded revision"
    );
    assert_eq!(stream_order_slides[0].text, "stale revision");
}

/// GH#1614, the other half: `SlideListWithText` states the presentation order, which
/// need not be the byte order the saves left the containers in.
#[test]
fn slides_are_numbered_by_presentation_order_not_stream_order() {
    let first_in_stream = container(RT_SLIDE, &text_chars_atom("appears second"));
    let second_in_stream = container(RT_SLIDE, &text_chars_atom("appears first"));

    let mut data = Vec::new();
    let stream_a = data.len() as u32;
    data.extend_from_slice(&first_in_stream);
    let stream_b = data.len() as u32;
    data.extend_from_slice(&second_in_stream);

    // Persist id 1 is the deck's first slide and lives LATER in the stream.
    let slide_list = container(
        RT_SLIDE_LIST_WITH_TEXT,
        &[slide_persist_atom(1), slide_persist_atom(2)].concat(),
    );
    let document = container(RT_DOCUMENT, &slide_list);
    let document_offset = data.len() as u32;
    data.extend_from_slice(&document);

    let dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[
        (1, stream_b),
        (2, stream_a),
        (20, document_offset),
    ]));
    let edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(0, dir_offset, 20));

    let live = live_slide_offsets(&data, &current_user_stream(edit_offset)).expect("the persist chain must resolve");
    assert_eq!(live.offsets, vec![stream_b as usize, stream_a as usize]);

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, Some(&live), &mut warnings).expect("record parsing should succeed");

    assert_eq!(
        slides.iter().map(|s| (s.number, s.text.as_str())).collect::<Vec<_>>(),
        vec![(1, "appears first"), (2, "appears second")],
        "slide numbers and order come from the slide list, not the byte order"
    );
}

/// xberg-io/xberg#1639: a `.ppt` stream is append-only, so a deck saved more than once
/// carries one `SlideListWithText` per save. The first one in the stream is the
/// **oldest** save's; the live save's list -- reached via `docPersistIdRef` on the
/// current `UserEditAtom` -- is the presentation's. Reproduces the issue's own example:
/// a second save adds a slide, and the first save's list must not be read instead.
#[test]
fn the_live_document_slide_list_is_read_not_the_first_one_in_the_stream() {
    let slide_a = container(RT_SLIDE, &text_chars_atom("Slide A"));
    let mut data = Vec::new();
    let slide_a_offset = data.len() as u32;
    data.extend_from_slice(&slide_a);

    let old_slide_list = container(RT_SLIDE_LIST_WITH_TEXT, &slide_persist_atom(4));
    let old_document = container(RT_DOCUMENT, &old_slide_list);
    let old_document_offset = data.len() as u32;
    data.extend_from_slice(&old_document);

    let old_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[(4, slide_a_offset), (1, old_document_offset)]));
    let old_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(0, old_dir_offset, 1));

    let slide_b = container(RT_SLIDE, &text_chars_atom("Slide B"));
    let slide_b_offset = data.len() as u32;
    data.extend_from_slice(&slide_b);

    let new_slide_list = container(
        RT_SLIDE_LIST_WITH_TEXT,
        &[slide_persist_atom(4), slide_persist_atom(5)].concat(),
    );
    let new_document = container(RT_DOCUMENT, &new_slide_list);
    let new_document_offset = data.len() as u32;
    data.extend_from_slice(&new_document);

    let new_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[(5, slide_b_offset), (1, new_document_offset)]));
    let new_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(old_edit_offset, new_dir_offset, 1));

    let live =
        live_slide_offsets(&data, &current_user_stream(new_edit_offset)).expect("the persist chain must resolve");
    assert_eq!(
        live.offsets,
        vec![slide_a_offset as usize, slide_b_offset as usize],
        "the live document's slide list names both persist ids 4 and 5, not just the first save's 4"
    );

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, Some(&live), &mut warnings).expect("record parsing should succeed");
    assert_eq!(slides.len(), 2, "the deck added a slide in its second save");
    assert_eq!(slides[0].text, "Slide A");
    assert_eq!(slides[1].text, "Slide B");
}

/// xberg-io/xberg#1639, the outline half: the outline-text harvest has the same
/// first-in-stream defect as the slide list -- every `SlideListWithText` met while
/// walking the whole stream contributed outline text, so a stale save's wording of a
/// slide's title could appear beside the live one. Only the live document's own list
/// may be harvested.
#[test]
fn outline_text_is_harvested_only_from_the_live_slide_list() {
    let slide1 = container(RT_SLIDE, &text_chars_atom("body text"));
    let mut data = Vec::new();
    let slide1_offset = data.len() as u32;
    data.extend_from_slice(&slide1);

    let mut old_outline = Vec::new();
    old_outline.extend_from_slice(&slide_persist_atom(1));
    old_outline.extend_from_slice(&text_chars_atom("Old Title"));
    let old_slide_list = container(RT_SLIDE_LIST_WITH_TEXT, &old_outline);
    let old_document = container(RT_DOCUMENT, &old_slide_list);
    let old_document_offset = data.len() as u32;
    data.extend_from_slice(&old_document);

    let old_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[(1, slide1_offset), (2, old_document_offset)]));
    let old_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(0, old_dir_offset, 2));

    let mut new_outline = Vec::new();
    new_outline.extend_from_slice(&slide_persist_atom(1));
    new_outline.extend_from_slice(&text_chars_atom("New Title"));
    let new_slide_list = container(RT_SLIDE_LIST_WITH_TEXT, &new_outline);
    let new_document = container(RT_DOCUMENT, &new_slide_list);
    let new_document_offset = data.len() as u32;
    data.extend_from_slice(&new_document);

    let new_dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[(1, slide1_offset), (2, new_document_offset)]));
    let new_edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(old_edit_offset, new_dir_offset, 2));

    let live =
        live_slide_offsets(&data, &current_user_stream(new_edit_offset)).expect("the persist chain must resolve");

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, Some(&live), &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 1);
    assert_eq!(
        slides[0].text, "New Title\nbody text",
        "only the live save's outline title is recovered"
    );
    assert!(
        !slides[0].text.contains("Old Title"),
        "a stale save's outline text must not reach the output"
    );
}

/// A deck whose chain cannot be read must keep the stream-order behaviour this
/// extractor had before, rather than losing slides to a partial answer.
#[test]
fn an_unreadable_persist_chain_falls_back_to_stream_order() {
    let mut data = Vec::new();
    data.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("one")));
    data.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("two")));

    assert!(
        live_slide_offsets(&data, &[]).is_none(),
        "an empty Current User stream resolves nothing"
    );
    assert!(
        live_slide_offsets(&data, &current_user_stream(9_999)).is_none(),
        "an offsetToCurrentEdit past the end of the stream resolves nothing"
    );

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");
    assert_eq!(slides.len(), 2, "stream order still yields both slides");
}

/// #87: `SlideListWithText` (0x0FF0) is a per-document container of
/// `SlidePersistAtom` outline-view entries -- it does not occur once per
/// slide, and (as in real files) commonly holds no text of its own. The
/// actual per-slide text lives in each `Slide` (0x03EE) container.
/// Segmenting on `SlideListWithText` merges every slide's text into a
/// single blob attributed to the wrong slide count; segmenting on
/// `Slide` keeps each slide's text separate.
#[test]
fn test_extract_texts_segments_on_slide_not_slide_list_with_text() {
    const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    const RT_SLIDE_PERSIST_ATOM: u16 = 0x03F3;

    // A SlideListWithText container holding only SlidePersistAtom entries
    // (no text), exactly as real files lay it out -- this used to be
    // mistaken for a slide boundary.
    let slide_persist_atom = record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0);
    let bogus_slwt = container(RT_SLIDE_LIST_WITH_TEXT, &slide_persist_atom);

    let slide1 = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let slide2 = container(RT_SLIDE, &text_chars_atom("Slide Two"));

    let mut data = Vec::new();
    data.extend_from_slice(&bogus_slwt);
    data.extend_from_slice(&slide1);
    data.extend_from_slice(&slide2);

    let mut warnings = Vec::new();
    let (slides, loose_texts, notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(
        slides.len(),
        2,
        "each Slide container is one slide, not each SlideListWithText"
    );
    assert_eq!(slides[0].number, 1);
    assert_eq!(slides[0].text, "Slide One");
    assert_eq!(slides[1].number, 2);
    assert_eq!(slides[1].text, "Slide Two");
    assert!(loose_texts.is_empty());
    assert!(notes.is_empty());
    assert!(warnings.is_empty(), "well-formed records should not warn: {warnings:?}");
}

/// xberg-io/xberg#1612: a legacy deck keeps a slide's title in the document-level
/// outline collection (`SlideListWithText`) and only the body in the slide's own drawing.
/// The walk collected text from `Slide` containers only, so every such title landed in
/// `loose_texts` -- which is discarded unless there are no slides at all -- and vanished.
///
/// This does NOT re-segment on `SlideListWithText`; see
/// `test_extract_texts_segments_on_slide_not_slide_list_with_text`, which still pins that.
/// Slides are still one-per-`Slide`; the outline text is attributed to them by the order of
/// the `SlidePersistAtom` entries that introduce each slide's outline records. ~keep
#[test]
fn test_extract_texts_recovers_titles_from_slide_list_with_text() {
    const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    const RT_SLIDE_PERSIST_ATOM: u16 = 0x03F3;

    let mut outline = Vec::new();
    outline.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    outline.extend_from_slice(&text_chars_atom("Search strategy development"));
    outline.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    outline.extend_from_slice(&text_chars_atom("Results and discussion"));
    let slwt = container(RT_SLIDE_LIST_WITH_TEXT, &outline);

    let slide1 = container(RT_SLIDE, &text_chars_atom("=> FILE HCAPLUS"));
    let slide2 = container(RT_SLIDE, &text_chars_atom("=> DISPLAY L1"));

    let mut data = Vec::new();
    data.extend_from_slice(&slwt);
    data.extend_from_slice(&slide1);
    data.extend_from_slice(&slide2);

    let mut warnings = Vec::new();
    let (slides, loose_texts, notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 2, "still one slide per Slide container");
    assert_eq!(slides[0].number, 1);
    assert_eq!(slides[0].text, "Search strategy development\n=> FILE HCAPLUS");
    assert_eq!(slides[1].number, 2);
    assert_eq!(slides[1].text, "Results and discussion\n=> DISPLAY L1");
    assert!(
        loose_texts.is_empty(),
        "outline text belongs to a slide, not to loose text"
    );
    assert!(notes.is_empty());
    assert!(warnings.is_empty(), "well-formed records should not warn: {warnings:?}");
}

/// A title that is *also* drawn on the slide canvas must appear once, not twice: the
/// reporter noted that decks where the title is drawn are exactly the ones whose titles
/// already survived, so recovering the outline copy must not double them. ~keep
#[test]
fn test_outline_title_already_drawn_on_the_slide_is_not_duplicated() {
    const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    const RT_SLIDE_PERSIST_ATOM: u16 = 0x03F3;

    let mut outline = Vec::new();
    outline.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    outline.extend_from_slice(&text_chars_atom("Title Slide"));
    let slwt = container(RT_SLIDE_LIST_WITH_TEXT, &outline);

    let mut slide_children = Vec::new();
    slide_children.extend_from_slice(&text_chars_atom("Title Slide"));
    slide_children.extend_from_slice(&text_chars_atom("With a subtitle"));
    let slide1 = container(RT_SLIDE, &slide_children);

    let mut data = Vec::new();
    data.extend_from_slice(&slwt);
    data.extend_from_slice(&slide1);

    let mut warnings = Vec::new();
    let (slides, _loose, _notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 1);
    assert_eq!(slides[0].text, "Title Slide\nWith a subtitle");
}

/// xberg-io/xberg#1635: the outline's own `TextHeaderAtom` says which run is the slide's
/// title (`Title` = 0) -- not the first line of whatever text ends up on the slide. A
/// body-typed atom (`Body` = 1) in the same outline entry must not be mistaken for one.
#[test]
fn test_extract_texts_reads_the_outline_title_type_not_the_first_line() {
    let mut outline = Vec::new();
    outline.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    outline.extend_from_slice(&text_header_atom(TEXT_TYPE_TITLE));
    outline.extend_from_slice(&text_chars_atom("Search strategy development"));
    outline.extend_from_slice(&text_header_atom(1)); // Body
    outline.extend_from_slice(&text_chars_atom("a body bullet, not the title"));
    let slwt = container(RT_SLIDE_LIST_WITH_TEXT, &outline);

    let slide1 = container(RT_SLIDE, &text_chars_atom("=> FILE HCAPLUS"));

    let mut data = Vec::new();
    data.extend_from_slice(&slwt);
    data.extend_from_slice(&slide1);

    let mut warnings = Vec::new();
    let (slides, _loose, _notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 1);
    assert_eq!(
        slides[0].title.as_deref(),
        Some("Search strategy development"),
        "the Title-typed atom is the title, not the body bullet or the drawing's first line"
    );
}

/// xberg-io/xberg#1635, negative control: `CenterTitle` is `6`, not `5` -- `5` is
/// `CenterBody`, ordinary body text. Reading the wrong value would silently promote a
/// slide's body to its title. ~keep
#[test]
fn test_extract_texts_treats_center_title_as_six_not_five() {
    let mut center_title = Vec::new();
    center_title.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    center_title.extend_from_slice(&text_header_atom(TEXT_TYPE_CENTER_TITLE));
    center_title.extend_from_slice(&text_chars_atom("Refworks"));
    let mut data = Vec::new();
    data.extend_from_slice(&container(RT_SLIDE_LIST_WITH_TEXT, &center_title));
    data.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("some body")));

    let mut warnings = Vec::new();
    let (slides, _, _) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");
    assert_eq!(
        slides[0].title.as_deref(),
        Some("Refworks"),
        "textType 6 (CenterTitle) is the centred-title placeholder"
    );

    let mut center_body = Vec::new();
    center_body.extend_from_slice(&record_header(0x0000, RT_SLIDE_PERSIST_ATOM, 0));
    center_body.extend_from_slice(&text_header_atom(5));
    center_body.extend_from_slice(&text_chars_atom("body, not a title"));
    let mut data5 = Vec::new();
    data5.extend_from_slice(&container(RT_SLIDE_LIST_WITH_TEXT, &center_body));
    data5.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("drawn body")));

    let mut warnings5 = Vec::new();
    let (slides5, _, _) =
        extract_texts_from_records(&data5, false, None, &mut warnings5).expect("record parsing should succeed");
    assert_eq!(
        slides5[0].title, None,
        "textType 5 is CenterBody, not CenterTitle -- must not become the title"
    );
}

/// xberg-io/xberg#1640: the notes master is also an `RT_NOTES` container --
/// `NotesAtom.slideIdRef == 0x80000000` -- and carries the notes page's layout
/// placeholder text, not a slide's speaker notes. It must never reach
/// `speaker_notes` or attach to any slide.
#[test]
fn should_not_treat_the_notes_master_placeholder_as_a_speaker_note() {
    let mut notes_master = Vec::new();
    notes_master.extend_from_slice(&notes_atom(NOTES_MASTER_SLIDE_ID_REF));
    notes_master.extend_from_slice(&text_chars_atom("Click to edit Master text styles"));
    let notes_master_container = container(RT_NOTES, &notes_master);

    let mut data = Vec::new();
    data.extend_from_slice(&notes_master_container);
    data.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("Slide One")));

    let mut warnings = Vec::new();
    let (slides, _loose, speaker_notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert!(
        speaker_notes.is_empty(),
        "the notes master's placeholder text must not become a speaker note: {speaker_notes:?}"
    );
    assert_eq!(slides.len(), 1);
    assert_eq!(
        slides[0].notes, None,
        "the master placeholder must not attach to slide 1 either"
    );
}

/// xberg-io/xberg#1640: notes must attach to the slide `NotesAtom.slideIdRef` names,
/// resolved through the live `SlidePersistAtom.slideId` -- not to the slide at the same
/// position among the deck's non-empty notes pages. Slide 1 has no notes; slide 2's
/// note must land on slide 2, not shift onto slide 1.
#[test]
fn should_attach_notes_to_the_slide_named_by_slide_id_ref_not_position() {
    let slide1 = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let slide2 = container(RT_SLIDE, &text_chars_atom("Slide Two"));

    let mut data = Vec::new();
    let slide1_offset = data.len() as u32;
    data.extend_from_slice(&slide1);
    let slide2_offset = data.len() as u32;
    data.extend_from_slice(&slide2);

    let mut notes2 = Vec::new();
    notes2.extend_from_slice(&notes_atom(101));
    notes2.extend_from_slice(&text_chars_atom("Notes for slide two"));
    data.extend_from_slice(&container(RT_NOTES, &notes2));

    let slide_list = container(
        RT_SLIDE_LIST_WITH_TEXT,
        &[
            slide_persist_atom_with_slide_id(10, 100),
            slide_persist_atom_with_slide_id(11, 101),
        ]
        .concat(),
    );
    let document = container(RT_DOCUMENT, &slide_list);
    let document_offset = data.len() as u32;
    data.extend_from_slice(&document);

    let dir_offset = data.len() as u32;
    data.extend_from_slice(&persist_directory(&[
        (10, slide1_offset),
        (11, slide2_offset),
        (99, document_offset),
    ]));
    let edit_offset = data.len() as u32;
    data.extend_from_slice(&user_edit_atom(0, dir_offset, 99));

    let live = live_slide_offsets(&data, &current_user_stream(edit_offset)).expect("the persist chain must resolve");

    let mut warnings = Vec::new();
    let (slides, _loose, speaker_notes) =
        extract_texts_from_records(&data, false, Some(&live), &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 2);
    assert_eq!(
        slides[0].notes, None,
        "slide 1 has no notes and must not inherit slide 2's by position"
    );
    assert_eq!(slides[1].notes.as_deref(), Some("Notes for slide two"));
    assert_eq!(speaker_notes, vec!["Notes for slide two".to_string()]);
}

/// A `Notes` container's text must not bleed into the slide that follows
/// it once its own byte range has ended.
#[test]
fn test_extract_texts_closes_notes_range_before_next_slide() {
    let notes = container(RT_NOTES, &text_chars_atom("Speaker notes"));
    let slide1 = container(RT_SLIDE, &text_chars_atom("Slide One"));

    let mut data = Vec::new();
    data.extend_from_slice(&notes);
    data.extend_from_slice(&slide1);

    let mut warnings = Vec::new();
    let (slides, loose_texts, speaker_notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(slides.len(), 1);
    assert_eq!(slides[0].number, 1);
    assert_eq!(slides[0].text, "Slide One");
    assert!(loose_texts.is_empty());
    assert_eq!(speaker_notes, vec!["Speaker notes".to_string()]);
}

/// #1418: a slide's number must come from its position among `RT_SLIDE`
/// containers, not from the position of a text block after joining and
/// re-splitting on `"\n\n"`. A slide with no text atoms must still get a
/// number instead of vanishing and shifting every later slide down.
#[test]
fn should_number_slides_by_persist_order_when_a_middle_slide_has_no_text() {
    let slide1 = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let slide2 = container(RT_SLIDE, &[]); // no text atoms at all
    let slide3 = container(RT_SLIDE, &text_chars_atom("Slide Three"));

    let mut data = Vec::new();
    data.extend_from_slice(&slide1);
    data.extend_from_slice(&slide2);
    data.extend_from_slice(&slide3);

    let mut warnings = Vec::new();
    let (slides, _loose_texts, _notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(
        slides.len(),
        3,
        "the empty middle slide must still produce a slide entry"
    );
    assert_eq!(slides[0].number, 1);
    assert_eq!(slides[0].text, "Slide One");
    assert_eq!(slides[1].number, 2);
    assert_eq!(
        slides[1].text, "",
        "a slide with no text atoms has empty text, not a missing entry"
    );
    assert_eq!(slides[2].number, 3);
    assert_eq!(slides[2].text, "Slide Three");
}

/// #1418 root-cause regression: a single slide whose own atoms, once
/// joined by `clean_ppt_text`'s newline mapping, contain an internal
/// `"\n\n"` (a text atom ending in a blank trailing paragraph, i.e. two
/// consecutive `\r` paragraph marks) must still be reported as exactly
/// one slide. The old algorithm re-split the whole document's text on
/// `"\n\n"`, so this single slide's own text was itself indistinguishable
/// from a slide boundary.
#[test]
fn should_keep_one_slide_entry_when_slide_text_contains_internal_blank_line() {
    let atom_with_trailing_blank_paragraph = text_chars_atom("Title\r\r");
    let atom_body = text_chars_atom("Body");
    let mut slide_children = Vec::new();
    slide_children.extend_from_slice(&atom_with_trailing_blank_paragraph);
    slide_children.extend_from_slice(&atom_body);
    let slide1 = container(RT_SLIDE, &slide_children);

    let mut data = Vec::new();
    data.extend_from_slice(&slide1);

    let mut warnings = Vec::new();
    let (slides, _loose_texts, _notes) =
        extract_texts_from_records(&data, false, None, &mut warnings).expect("record parsing should succeed");

    assert_eq!(
        slides.len(),
        1,
        "one Slide container is one slide, however its joined text looks"
    );
    assert_eq!(slides[0].number, 1);
    assert_eq!(
        slides[0].text, "Title\n\nBody",
        "the slide's own text legitimately contains an internal blank line"
    );
}
