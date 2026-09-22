use super::super::*;
use super::text_records::{container, record_header, text_chars_atom};

/// Build a raster `OfficeArtBlip` record with a single 16-byte UID
/// (MS-ODRAW 2.2.27-2.2.29 "one UID" layout: `rgbUid1(16) + tag(1) +
/// BLIPFileData`). `rec_instance` must be even per spec (e.g. JPEG
/// 0x46A, PNG 0x6E0, DIB 0x7A8).
fn blip_record_one_uid(rec_instance: u16, rec_type: u16, picture_bytes: &[u8]) -> Vec<u8> {
    assert_eq!(rec_instance & 0x1, 0, "one-UID recInstance must be even");
    let rec_ver_instance = rec_instance << 4;
    let rec_len = (17 + picture_bytes.len()) as u32;
    let mut buf = record_header(rec_ver_instance, rec_type, rec_len);
    buf.extend_from_slice(&[0u8; 16]);
    buf.push(0xFF);
    buf.extend_from_slice(picture_bytes);
    buf
}

/// Build a raster `OfficeArtBlip` record with two 16-byte UIDs
/// ("two UID" layout: `rgbUid1(16) + rgbUid2(16) + tag(1) +
/// BLIPFileData`). `rec_instance` must be odd per spec (e.g. JPEG
/// 0x46B, PNG 0x6E1, DIB 0x7A9).
fn blip_record_two_uid(rec_instance: u16, rec_type: u16, picture_bytes: &[u8]) -> Vec<u8> {
    assert_eq!(rec_instance & 0x1, 1, "two-UID recInstance must be odd");
    let rec_ver_instance = rec_instance << 4;
    let rec_len = (33 + picture_bytes.len()) as u32;
    let mut buf = record_header(rec_ver_instance, rec_type, rec_len);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0xFF);
    buf.extend_from_slice(picture_bytes);
    buf
}

#[test]
fn should_extract_jpeg_bytes_when_pictures_stream_has_one_uid_jpeg_blip() {
    let picture = b"\xFF\xD8\xFFfake-jpeg-payload";
    let data = blip_record_one_uid(0x46A, RT_BLIP_JPEG, picture);

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].format, "jpeg");
    assert_eq!(images[0].image_index, 0);
    assert_eq!(&images[0].data[..], &picture[..]);
    assert!(warnings.is_empty());
}

#[test]
fn should_extract_png_bytes_when_pictures_stream_has_two_uid_png_blip() {
    let picture = b"\x89PNG\r\n\x1a\nfake-png-payload";
    let data = blip_record_two_uid(0x6E1, RT_BLIP_PNG, picture);

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].format, "png");
    assert_eq!(&images[0].data[..], &picture[..]);
    assert!(warnings.is_empty());
}

#[test]
fn should_extract_dib_bytes_and_tag_format_dib_when_pictures_stream_has_dib_blip() {
    let picture = b"fake-dib-bitmap-payload";
    let data = blip_record_one_uid(0x7A8, RT_BLIP_DIB, picture);

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].format, "dib");
    assert_eq!(&images[0].data[..], &picture[..]);
}

#[test]
fn should_assign_sequential_image_index_when_pictures_stream_has_multiple_blips() {
    let jpeg = blip_record_one_uid(0x46A, RT_BLIP_JPEG, b"jpeg-one");
    let png = blip_record_one_uid(0x6E0, RT_BLIP_PNG, b"png-two");

    let mut data = Vec::new();
    data.extend_from_slice(&jpeg);
    data.extend_from_slice(&png);

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert_eq!(images.len(), 2);
    assert_eq!(images[0].image_index, 0);
    assert_eq!(images[0].format, "jpeg");
    assert_eq!(images[1].image_index, 1);
    assert_eq!(images[1].format, "png");
}

#[test]
fn should_skip_non_blip_records_when_walking_pictures_stream() {
    // An arbitrary non-blip OfficeArt record (a group shape record,
    // 0xF003) sitting between two real blips must not be mistaken for a
    // picture and must not stop the walk.
    const RT_UNRELATED: u16 = 0xF003;
    let unrelated = record_header(0x0000, RT_UNRELATED, 4)
        .into_iter()
        .chain([1, 2, 3, 4])
        .collect::<Vec<u8>>();
    let jpeg = blip_record_one_uid(0x46A, RT_BLIP_JPEG, b"real-jpeg");

    let mut data = Vec::new();
    data.extend_from_slice(&unrelated);
    data.extend_from_slice(&jpeg);

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].format, "jpeg");
}

/// Safety: a record whose declared `recLen` overruns the remaining
/// buffer must never panic or over-read -- the walk stops and a
/// diagnostic warning is recorded instead.
#[test]
fn should_stop_without_panicking_when_blip_declares_length_past_buffer_end() {
    let mut data = record_header(0x46A << 4, RT_BLIP_JPEG, u32::MAX);
    data.extend_from_slice(&[0u8; 4]); // far short of the declared recLen

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert!(images.is_empty());
    assert!(
        warnings.iter().any(|w| w.message.contains("truncated")),
        "expected a truncation warning, got: {warnings:?}"
    );
}

/// Safety: a blip record declaring fewer bytes than its own UID header
/// requires must be skipped, not underflow-subtracted into a bogus
/// picture length.
#[test]
fn should_skip_and_warn_when_blip_declared_length_is_shorter_than_uid_header() {
    // recLen = 5, far short of the 17-byte one-UID header.
    let data = record_header(0x46A << 4, RT_BLIP_JPEG, 5)
        .into_iter()
        .chain([0u8; 5])
        .collect::<Vec<u8>>();

    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&data, &Default::default(), &mut warnings);

    assert!(images.is_empty());
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("shorter than its UID header")),
        "expected a UID-header-too-short warning, got: {warnings:?}"
    );
}

#[test]
fn should_return_no_images_when_pictures_stream_is_empty() {
    let mut warnings = Vec::new();
    let images = extract_pictures_from_stream(&[], &Default::default(), &mut warnings);
    assert!(images.is_empty());
    assert!(warnings.is_empty());
}

/// Build a minimal OLE/CFB container with a "PowerPoint Document" stream
/// and, optionally, a "Pictures" stream, mirroring what a real `.ppt`
/// looks like closely enough to drive `extract_ppt_text_with_options`
/// end-to-end. `test_documents/ppt/simple.ppt` has a `Pictures` stream
/// but it is empty (verified: 0 bytes), so this synthetic container is
/// the only way to exercise the `/Pictures` read path with real blips.
fn build_test_ppt_ole(ppt_document_stream: &[u8], pictures_stream: Option<&[u8]>) -> Vec<u8> {
    use std::io::Write;
    let cursor = Cursor::new(Vec::new());
    let mut comp = cfb::CompoundFile::create(cursor).expect("create in-memory OLE container");
    comp.create_stream("/PowerPoint Document")
        .expect("create PowerPoint Document stream")
        .write_all(ppt_document_stream)
        .expect("write PowerPoint Document stream");
    if let Some(pictures) = pictures_stream {
        comp.create_stream("/Pictures")
            .expect("create Pictures stream")
            .write_all(pictures)
            .expect("write Pictures stream");
    }
    comp.into_inner().into_inner()
}

#[test]
fn should_populate_images_when_pictures_stream_has_a_blip_and_extract_images_is_true() {
    let ppt_stream = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let picture = b"\xFF\xD8\xFFsynthetic-jpeg-bytes";
    let pictures_stream = blip_record_one_uid(0x46A, RT_BLIP_JPEG, picture);
    let content = build_test_ppt_ole(&ppt_stream, Some(&pictures_stream));

    let result = extract_ppt_text_with_options(&content, false, true).expect("synthetic OLE container should parse");

    assert_eq!(result.images.len(), 1);
    assert_eq!(result.images[0].format, "jpeg");
    assert_eq!(result.images[0].data.len(), picture.len());
    assert_eq!(&result.images[0].data[..], &picture[..]);
}

#[test]
fn should_return_no_images_when_extract_images_is_false() {
    let ppt_stream = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let pictures_stream = blip_record_one_uid(0x46A, RT_BLIP_JPEG, b"jpeg-bytes");
    let content = build_test_ppt_ole(&ppt_stream, Some(&pictures_stream));

    let result = extract_ppt_text_with_options(&content, false, false).expect("synthetic OLE container should parse");

    assert!(
        result.images.is_empty(),
        "extract_images=false must skip the Pictures stream entirely"
    );
}

/// REV-CB regression for GH#1687 (shares the GH#1662/GH#1686 fix): an OCR-only
/// `ExtractionConfig` (no `images.extract_images`, no captioning, no QR codes) must
/// still read a `.ppt` OLE container's embedded image out of its `Pictures` stream,
/// not skip it. `needs_image_data` gained the OCR disjunct that makes this true
/// (#1662); PPT shares that predicate with DOCX, HTML and PPTX through
/// `PptExtractor::extract_content`'s `config.needs_image_data()` call (see
/// `extractors::ppt`), but until now nothing exercised that call site directly.
/// Before the fix, `extract_images` stayed `false` for an OCR-only config, so
/// `extract_ppt_text_with_options` never read `/Pictures` at all and
/// `InternalDocument::images` stayed empty, silently, with no warning -- the same
/// shape `should_return_no_images_when_extract_images_is_false` above proves at the
/// lower `extract_ppt_text_with_options(bool)` layer. This test lives here rather
/// than in `extractors::ppt`'s own test module because it reuses `build_test_ppt_ole`
/// and the blip/record builders already proven above, instead of a second hand-rolled
/// OLE/CFB writer for the same container shape.
#[tokio::test]
async fn should_read_real_embedded_image_bytes_for_ocr_only_config_at_extract_content() {
    use crate::core::config::{ExtractionConfig, OcrConfig};
    use crate::core::mime::LEGACY_POWERPOINT_MIME_TYPE;
    use crate::extractors::ppt::PptExtractor;
    use crate::plugins::InternalDocumentExtractor;

    let ppt_stream = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let picture = b"\xFF\xD8\xFFsynthetic-jpeg-bytes";
    let pictures_stream = blip_record_one_uid(0x46A, RT_BLIP_JPEG, picture);
    let content = build_test_ppt_ole(&ppt_stream, Some(&pictures_stream));

    let config = ExtractionConfig {
        ocr: Some(OcrConfig::default()),
        ..Default::default()
    };

    let extractor = PptExtractor::new();
    let internal_doc = extractor
        .extract_content(&content, LEGACY_POWERPOINT_MIME_TYPE, &config)
        .await
        .expect("synthetic OLE container must extract");

    assert_eq!(internal_doc.images.len(), 1, "the single blip must yield one image");
    assert_eq!(
        internal_doc.images[0].data.as_ref(),
        &picture[..],
        "an OCR-only config must still read the real embedded-image bytes, not skip the Pictures stream"
    );
}

#[test]
fn should_return_no_images_when_pictures_stream_is_absent() {
    let ppt_stream = container(RT_SLIDE, &text_chars_atom("Slide One"));
    let content = build_test_ppt_ole(&ppt_stream, None);

    let result = extract_ppt_text_with_options(&content, false, true).expect("synthetic OLE container should parse");

    assert!(result.images.is_empty());
}

/// Build an `msofbtBSE` whose `foDelay` names `pictures_offset`. Only `foDelay` is
/// read; the surrounding documented fields are present so offsets are realistic.
fn bse_record(pictures_offset: u32) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(&[0x06, 0x06]); // btWin32, btMacOS
    content.extend_from_slice(&[0u8; 16]); // rgbUid
    content.extend_from_slice(&[0u8; 2]); // tag
    content.extend_from_slice(&0u32.to_le_bytes()); // size
    content.extend_from_slice(&1u32.to_le_bytes()); // cRef
    content.extend_from_slice(&pictures_offset.to_le_bytes()); // foDelay
    content.extend_from_slice(&[0u8; 4]); // unused1..3, cbName
    let mut buf = record_header(0x0000, MSOFBT_BSE, content.len() as u32);
    buf.extend_from_slice(&content);
    buf
}

/// Build an `msofbtOPT` property table carrying one `pib` entry.
fn opt_with_pib(pib: u32) -> Vec<u8> {
    let mut content = Vec::new();
    // `fBid` (bit 14) set, as a real picture shape writes it.
    content.extend_from_slice(&(MSO_PROPERTY_PIB | 0x4000).to_le_bytes());
    content.extend_from_slice(&pib.to_le_bytes());
    let mut buf = record_header(1 << 4, MSOFBT_OPT[0], content.len() as u32);
    buf.extend_from_slice(&content);
    buf
}

/// #1620: a picture's slide comes from the drawing that references it, not from the
/// `Pictures` stream, which is in save order and names no slide at all.
#[test]
fn should_attribute_each_picture_to_the_slide_whose_drawing_references_it() {
    let first_picture = b"\xFF\xD8\xFFfirst-picture";
    let second_picture = b"\xFF\xD8\xFFsecond";
    let first_blip = blip_record_one_uid(0x46A, RT_BLIP_JPEG, first_picture);
    let second_blip = blip_record_one_uid(0x46A, RT_BLIP_JPEG, second_picture);
    let second_offset = first_blip.len() as u32;
    let mut pictures_stream = first_blip.clone();
    pictures_stream.extend_from_slice(&second_blip);

    // `pib` is 1-based: 1 -> the blip at offset 0, 2 -> the blip after it. ~keep
    let mut bstore = bse_record(0);
    bstore.extend_from_slice(&bse_record(second_offset));
    let drawing_group = container(0xF001, &bstore);

    // Slide 1 shows the *second* blip and slide 2 the first, so a passing test cannot
    // be explained by the stream order the walk would otherwise fall back to.
    let mut stream = drawing_group;
    stream.extend_from_slice(&container(RT_SLIDE, &opt_with_pib(2)));
    let mut slide_two = text_chars_atom("Slide Two");
    slide_two.extend_from_slice(&opt_with_pib(1));
    stream.extend_from_slice(&container(RT_SLIDE, &slide_two));

    let content = build_test_ppt_ole(&stream, Some(&pictures_stream));
    let result = extract_ppt_text_with_options(&content, false, true).expect("synthetic deck should parse");

    assert_eq!(result.images.len(), 2, "both blips must still be extracted");
    assert_eq!(&result.images[0].data[..], &first_picture[..]);
    assert_eq!(
        result.images[0].page_number,
        Some(2),
        "first blip is referenced by slide 2"
    );
    assert_eq!(&result.images[1].data[..], &second_picture[..]);
    assert_eq!(
        result.images[1].page_number,
        Some(1),
        "second blip is referenced by slide 1"
    );
}

/// #1620: a blip in `Pictures` that no live shape displays keeps the pre-fix
/// behaviour -- still extracted, but attributed to no slide.
#[test]
fn should_leave_a_picture_no_shape_references_without_a_slide_number() {
    let picture = b"\xFF\xD8\xFForphan";
    let pictures_stream = blip_record_one_uid(0x46A, RT_BLIP_JPEG, picture);
    let mut stream = container(0xF001, &bse_record(0));
    stream.extend_from_slice(&container(RT_SLIDE, &text_chars_atom("Slide One")));

    let content = build_test_ppt_ole(&stream, Some(&pictures_stream));
    let result = extract_ppt_text_with_options(&content, false, true).expect("synthetic deck should parse");

    assert_eq!(result.images.len(), 1);
    assert_eq!(result.images[0].page_number, None);
}

/// #1620: a `pib` pointing past the end of the `BStoreContainer` is a corrupt deck,
/// not a panic -- the picture simply resolves to no slide.
#[test]
fn should_ignore_a_blip_index_beyond_the_blip_store() {
    let picture = b"\xFF\xD8\xFFonly";
    let pictures_stream = blip_record_one_uid(0x46A, RT_BLIP_JPEG, picture);
    let mut stream = container(0xF001, &bse_record(0));
    stream.extend_from_slice(&container(RT_SLIDE, &opt_with_pib(99)));

    let content = build_test_ppt_ole(&stream, Some(&pictures_stream));
    let result = extract_ppt_text_with_options(&content, false, true).expect("synthetic deck should parse");

    assert_eq!(result.images.len(), 1);
    assert_eq!(result.images[0].page_number, None);
}
