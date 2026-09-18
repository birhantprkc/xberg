//! Regression test for https://github.com/xberg-io/xberg/issues/1660
//!
//! A legacy `.ppt` carrying a Word table inserted as an OLE object lost that table
//! completely: the object's storage (`ExOleObjStg`) was walked over as opaque bytes, so the
//! slide came out as its title alone. The fixture is a three-slide deck whose second slide
//! holds an embedded Word document with a 3x3 table (built from an ODP with LibreOffice's
//! PowerPoint 97 export, which converts the Writer object to a Word OLE storage). ~keep

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "office")]

use xberg::core::config::{ExtractInput, ExtractionConfig};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/ppt/embedded_word_object.ppt"
    ))
    .expect("fixture must exist")
}

fn input() -> ExtractInput {
    ExtractInput::from_bytes(
        fixture(),
        "application/vnd.ms-powerpoint",
        Some("embedded_word_object.ppt".to_string()),
    )
}

#[tokio::test]
async fn should_extract_the_embedded_word_table_as_a_child_attributed_to_its_slide() {
    let config = ExtractionConfig::default();

    let result = xberg::extract(input(), &config).await.expect("extract");
    let document = result.results.first().expect("one document");

    assert!(
        document.content.contains("Compounds table"),
        "the slide's own title must still be there: {}",
        document.content
    );
    let children = document
        .children
        .as_ref()
        .expect("the embedded Word object must be extracted as a child");
    assert_eq!(children.len(), 1, "{children:?}");
    let child = &children[0];
    assert_eq!(child.path, "slide2/oleObject1.bin");
    assert_eq!(child.mime_type, "application/msword");
    let text = &child.result.content;
    for cell in [
        "COMMON NAME",
        "AGGREGATION STATE",
        "USES",
        "hexogen",
        "Iron pentacarbonyl",
        "Catalyst",
    ] {
        assert!(
            text.contains(cell),
            "child content must carry the table cell {cell:?}: {text}"
        );
    }
    assert!(
        document
            .processing_warnings
            .iter()
            .all(|w| w.source != "ppt_embedded_objects"),
        "{:?}",
        document.processing_warnings
    );
}

/// `max_archive_depth = 0` is the caller's way of refusing recursion into containers; the
/// legacy path must honour it the way the `.pptx` path does.
#[tokio::test]
async fn should_not_recurse_into_embedded_objects_when_archive_depth_is_zero() {
    let config = ExtractionConfig {
        max_archive_depth: 0,
        ..Default::default()
    };

    let result = xberg::extract(input(), &config).await.expect("extract");
    let document = result.results.first().expect("one document");

    assert!(document.children.is_none(), "{:?}", document.children);
}
