use super::*;

#[test]
fn test_internal_element_id_deterministic() {
    let id1 = InternalElementId::generate("heading", "Introduction", Some(1), 0);
    let id2 = InternalElementId::generate("heading", "Introduction", Some(1), 0);
    assert_eq!(id1, id2);
}

#[test]
fn test_internal_element_id_differs_by_index() {
    let id1 = InternalElementId::generate("paragraph", "Same text", Some(1), 0);
    let id2 = InternalElementId::generate("paragraph", "Same text", Some(1), 1);
    assert_ne!(id1, id2);
}

#[test]
fn test_internal_element_id_format() {
    let id = InternalElementId::generate("title", "Hello", None, 0);
    assert!(id.as_str().starts_with("ie-"));
    assert_eq!(id.as_str().len(), 3 + 12);
}

#[test]
fn test_element_kind_discriminant() {
    assert_eq!(ElementKind::Title.discriminant(), "title");
    assert_eq!(ElementKind::Heading { level: 2 }.discriminant(), "heading");
    assert_eq!(ElementKind::ListStart { ordered: true }.discriminant(), "list_start");
}

#[test]
fn test_container_markers() {
    assert!(ElementKind::ListStart { ordered: false }.is_container_start());
    assert!(ElementKind::ListEnd.is_container_end());
    assert!(!ElementKind::Paragraph.is_container_start());
    assert_eq!(ElementKind::QuoteStart.matching_end(), Some(ElementKind::QuoteEnd));
}

#[test]
fn test_internal_document_push() {
    let mut doc = InternalDocument::new("markdown");
    let elem = InternalElement::text(ElementKind::Paragraph, "Hello world", 0);
    let idx = doc.push_element(elem);
    assert_eq!(idx, 0);
    assert_eq!(doc.elements.len(), 1);
    assert_eq!(doc.elements[0].text, "Hello world");
}

#[test]
fn public_attributes_preserve_explicit_empty_map() {
    let mut element = InternalElement::text(ElementKind::Paragraph, "text", 0);
    element.attributes = Some(AHashMap::new());

    assert_eq!(element.public_attributes(), Some(std::collections::HashMap::new()));
}

#[cfg(all(feature = "pdf", any(feature = "ocr", feature = "ocr-pipeline")))]
#[test]
fn public_attributes_hide_internal_image_ocr_suppression() {
    let mut element = InternalElement::text(ElementKind::Image { image_index: 0 }, "", 0);
    element.suppress_image_ocr_rendering();

    assert!(element.public_attributes().is_none());
    assert!(!element.should_render_image_ocr());
}

/// #### FAILS against unfixed code
/// `set_list_item_source_label`/`list_item_source_label` do not exist yet
/// on unfixed `InternalElement` -- this test does not compile without the
/// fix. Once the fix lands, it proves two things a bare
/// `ElementKind::ListItem { ordered: true }` cannot: the literal marker
/// text round-trips unchanged, and -- unlike the OCR-suppression
/// attribute -- it is NOT filtered out of `public_attributes()`, so it
/// reaches the public `DocumentStructure` tree via `DocumentNode::attributes`.
#[cfg(feature = "pdf")]
#[test]
fn list_item_source_label_round_trips_and_stays_public() {
    let mut element = InternalElement::text(ElementKind::ListItem { ordered: false }, "General Provisions.", 1);
    assert_eq!(element.list_item_source_label(), None);

    element.set_list_item_source_label("B.");

    assert_eq!(element.list_item_source_label(), Some("B."));
    assert_eq!(
        element.public_attributes(),
        Some(std::collections::HashMap::from([(
            "list_marker".to_string(),
            "B.".to_string()
        )]))
    );
}

/// An empty label is a caller bug (e.g. a marker-strip that removed
/// nothing), not a real source marker -- `set_list_item_source_label`
/// must not manufacture a spurious attribute for it.
#[cfg(feature = "pdf")]
#[test]
fn list_item_source_label_ignores_an_empty_label() {
    let mut element = InternalElement::text(ElementKind::ListItem { ordered: true }, "item text", 1);
    element.set_list_item_source_label("");
    assert_eq!(element.list_item_source_label(), None);
    assert_eq!(element.attributes, None);
}

#[cfg(any(feature = "ocr", feature = "pdf", paddle_ocr, feature = "xml", feature = "office"))]
#[test]
fn test_internal_element_builder_pattern() {
    let elem = InternalElement::text(ElementKind::Heading { level: 2 }, "Methods", 1)
        .with_page(3)
        .with_anchor("methods")
        .with_layer(ContentLayer::Body);

    assert_eq!(elem.text, "Methods");
    assert_eq!(elem.page, Some(3));
    assert_eq!(elem.anchor, Some("methods".to_string()));
    assert_eq!(elem.depth, 1);
}

#[test]
fn test_relationship_kind_serde() {
    let kind = RelationshipKind::FootnoteReference;
    let json = serde_json::to_string(&kind).unwrap();
    assert_eq!(json, "\"footnote_reference\"");

    let parsed: RelationshipKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, kind);
}

/// Verify that `InternalDocument` round-trips through serde JSON without loss.
///
/// This is the primary correctness gate for foreign-language plugin support:
/// Python/TypeScript/Ruby implementations of `DocumentExtractor` construct
/// an `InternalDocument` as JSON and pass it across the FFI boundary.
#[test]
fn should_round_trip_through_serde_json() {
    let mut doc = InternalDocument::new("pdf");
    doc.mime_type = "application/pdf".to_string();

    let title = InternalElement::text(ElementKind::Title, "Test Document", 0);
    doc.push_element(title);

    let heading = InternalElement::text(ElementKind::Heading { level: 2 }, "Introduction", 1);
    doc.push_element(heading);

    let para = InternalElement::text(ElementKind::Paragraph, "Body text here.", 1);
    doc.push_element(para);

    let list_start = InternalElement::text(ElementKind::ListStart { ordered: true }, "", 1);
    doc.push_element(list_start);
    let item = InternalElement::text(ElementKind::ListItem { ordered: true }, "First item", 2);
    doc.push_element(item);
    let list_end = InternalElement::text(ElementKind::ListEnd, "", 1);
    doc.push_element(list_end);

    let code = InternalElement::text(ElementKind::Code, "fn main() {}", 0);
    doc.push_element(code);

    let pb = InternalElement::text(ElementKind::PageBreak, "", 0);
    doc.push_element(pb);

    let img_elem = InternalElement::text(ElementKind::Image { image_index: 0 }, "", 0);
    doc.push_element(img_elem);

    let ocr = InternalElement::text(
        ElementKind::OcrText {
            level: OcrElementLevel::Word,
        },
        "scanned word",
        0,
    );
    doc.push_element(ocr);

    doc.push_relationship(Relationship {
        source: 0,
        target: RelationshipTarget::Index(2),
        kind: RelationshipKind::FootnoteReference,
    });
    doc.push_relationship(Relationship {
        source: 1,
        target: RelationshipTarget::Key("introduction".to_string()),
        kind: RelationshipKind::CrossReference,
    });

    let json = serde_json::to_string(&doc).expect("serialize InternalDocument");
    let restored: InternalDocument = serde_json::from_str(&json).expect("deserialize InternalDocument");

    assert_eq!(restored.source_format, doc.source_format);
    assert_eq!(restored.mime_type, doc.mime_type);
    assert_eq!(restored.elements.len(), doc.elements.len());
    assert_eq!(restored.relationships.len(), doc.relationships.len());

    assert_eq!(restored.elements[0].kind, ElementKind::Title);
    assert_eq!(restored.elements[1].kind, ElementKind::Heading { level: 2 });
    assert_eq!(restored.elements[4].kind, ElementKind::ListItem { ordered: true });
    assert_eq!(restored.elements[8].kind, ElementKind::Image { image_index: 0 });
    assert_eq!(
        restored.elements[9].kind,
        ElementKind::OcrText {
            level: OcrElementLevel::Word
        }
    );

    assert_eq!(restored.relationships[0].target, RelationshipTarget::Index(2));
    assert_eq!(
        restored.relationships[1].target,
        RelationshipTarget::Key("introduction".to_string())
    );

    assert_eq!(restored.elements[0].id, doc.elements[0].id);

    assert_eq!(restored.elements[0].layer, ContentLayer::Body);
}

/// Cover all 27 `ElementKind` variants through a serde JSON round-trip.
///
/// Every variant must be constructed, serialised, and deserialised; the
/// `kind` field is then asserted on each restored element so that a missing
/// or mis-tagged variant surfaces immediately.
#[test]
fn should_cover_all_element_kind_variants() {
    let mut doc = InternalDocument::new("test");

    doc.push_element(InternalElement::text(ElementKind::Title, "T", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Title);

    doc.push_element(InternalElement::text(ElementKind::Heading { level: 1 }, "H1", 1));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Heading { level: 1 });

    doc.push_element(InternalElement::text(ElementKind::Paragraph, "P", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Paragraph);

    doc.push_element(InternalElement::text(ElementKind::ListItem { ordered: false }, "li", 2));
    assert_eq!(
        doc.elements.last().unwrap().kind,
        ElementKind::ListItem { ordered: false }
    );

    doc.push_element(InternalElement::text(ElementKind::Code, "x=1", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Code);

    doc.push_element(InternalElement::text(ElementKind::Formula, "E=mc^2", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Formula);

    doc.push_element(InternalElement::text(ElementKind::FootnoteDefinition, "note text", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::FootnoteDefinition);

    doc.push_element(InternalElement::text(ElementKind::FootnoteRef, "1", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::FootnoteRef);

    doc.push_element(InternalElement::text(ElementKind::Citation, "Smith 2020", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Citation);

    doc.push_element(InternalElement::text(ElementKind::Slide { number: 3 }, "slide 3", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Slide { number: 3 });

    doc.push_element(InternalElement::text(ElementKind::DefinitionTerm, "term", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::DefinitionTerm);

    doc.push_element(InternalElement::text(ElementKind::DefinitionDescription, "desc", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::DefinitionDescription);

    doc.push_element(InternalElement::text(ElementKind::Admonition, "Note:", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Admonition);

    doc.push_element(InternalElement::text(ElementKind::RawBlock, "<raw/>", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::RawBlock);

    doc.push_element(InternalElement::text(ElementKind::MetadataBlock, "---", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::MetadataBlock);

    doc.push_element(InternalElement::text(ElementKind::ListStart { ordered: true }, "", 0));
    assert_eq!(
        doc.elements.last().unwrap().kind,
        ElementKind::ListStart { ordered: true }
    );

    doc.push_element(InternalElement::text(ElementKind::ListEnd, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::ListEnd);

    doc.push_element(InternalElement::text(ElementKind::QuoteStart, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::QuoteStart);

    doc.push_element(InternalElement::text(ElementKind::QuoteEnd, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::QuoteEnd);

    doc.push_element(InternalElement::text(ElementKind::GroupStart, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::GroupStart);

    doc.push_element(InternalElement::text(ElementKind::GroupEnd, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::GroupEnd);

    doc.push_element(InternalElement::text(ElementKind::Table { table_index: 0 }, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Table { table_index: 0 });

    doc.push_element(InternalElement::text(ElementKind::Image { image_index: 1 }, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::Image { image_index: 1 });

    doc.push_element(InternalElement::text(ElementKind::PageBreak, "", 0));
    assert_eq!(doc.elements.last().unwrap().kind, ElementKind::PageBreak);

    for level in [
        OcrElementLevel::Word,
        OcrElementLevel::Line,
        OcrElementLevel::Block,
        OcrElementLevel::Page,
    ] {
        doc.push_element(InternalElement::text(ElementKind::OcrText { level }, "ocr", 0));
        assert_eq!(doc.elements.last().unwrap().kind, ElementKind::OcrText { level });
    }

    let json = serde_json::to_string(&doc).expect("serialize all-variant InternalDocument");
    let restored: InternalDocument = serde_json::from_str(&json).expect("deserialize all-variant InternalDocument");

    assert_eq!(restored.elements.len(), doc.elements.len());

    assert_eq!(restored.elements[0].kind, ElementKind::Title);
    assert_eq!(restored.elements[5].kind, ElementKind::Formula);
    assert_eq!(restored.elements[9].kind, ElementKind::Slide { number: 3 });
    assert_eq!(restored.elements[14].kind, ElementKind::MetadataBlock);
    assert_eq!(restored.elements[16].kind, ElementKind::ListEnd);
    assert_eq!(restored.elements[17].kind, ElementKind::QuoteStart);
    assert_eq!(restored.elements[18].kind, ElementKind::QuoteEnd);
    assert_eq!(restored.elements[19].kind, ElementKind::GroupStart);
    assert_eq!(restored.elements[20].kind, ElementKind::GroupEnd);
    assert_eq!(restored.elements[21].kind, ElementKind::Table { table_index: 0 });
    assert_eq!(restored.elements[23].kind, ElementKind::PageBreak);
    assert_eq!(
        restored.elements[24].kind,
        ElementKind::OcrText {
            level: OcrElementLevel::Word
        }
    );
    assert_eq!(
        restored.elements[27].kind,
        ElementKind::OcrText {
            level: OcrElementLevel::Page
        }
    );
}

/// Verify that both `RelationshipTarget` variants survive a serde JSON
/// round-trip when carried inside a `Relationship`.
#[test]
fn should_round_trip_relationship_targets() {
    let mut doc = InternalDocument::new("test");
    doc.push_element(InternalElement::text(ElementKind::Paragraph, "source", 0));
    doc.push_element(InternalElement::text(ElementKind::Paragraph, "target", 0));

    doc.push_relationship(Relationship {
        source: 0,
        target: RelationshipTarget::Index(1),
        kind: RelationshipKind::CrossReference,
    });
    doc.push_relationship(Relationship {
        source: 0,
        target: RelationshipTarget::Key("anchor-abc".to_string()),
        kind: RelationshipKind::FootnoteReference,
    });

    let json = serde_json::to_string(&doc).expect("serialize RelationshipTarget variants");
    let restored: InternalDocument = serde_json::from_str(&json).expect("deserialize RelationshipTarget variants");

    assert_eq!(restored.relationships.len(), 2);
    assert_eq!(restored.relationships[0].target, RelationshipTarget::Index(1));
    assert_eq!(
        restored.relationships[1].target,
        RelationshipTarget::Key("anchor-abc".to_string())
    );
    assert_eq!(restored.relationships[0].kind, RelationshipKind::CrossReference);
    assert_eq!(restored.relationships[1].kind, RelationshipKind::FootnoteReference);
}
