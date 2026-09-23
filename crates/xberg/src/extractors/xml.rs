//! XML extractor.

use crate::Result;
use crate::core::config::ExtractionConfig;
use crate::core::mime::{KML_MIME_TYPE, ODG_FLAT_MIME_TYPE};
use crate::extraction::xml::{parse_xml, parse_xml_svg};
use crate::extractors::SyncExtractor;
use crate::extractors::security::SecurityBudget;
use crate::plugins::{InternalDocumentExtractor, Plugin};
use crate::types::internal::{ElementKind, InternalDocument, InternalElement};
use crate::types::metadata::Metadata;
use ahash::AHashMap;
use async_trait::async_trait;

/// `ProcessingWarning::source` used for every degradation reported by this extractor.
const XML_WARNING_SOURCE: &str = "xml";
const MAX_XML_HEADING_LEVEL: u16 = 6;

fn heading_level(depth: u16) -> u8 {
    depth.saturating_add(1).min(MAX_XML_HEADING_LEVEL) as u8
}

/// Whether the caller actually asked for the recovered diagram, i.e. the
/// request's output format resolves to the `dot` renderer.
///
/// `OutputFormat::Custom` is how any renderer, built-in or registered, is
/// selected (see `plugins::registry::RendererRegistry`), so matching the
/// renderer name here is the same test `derive_extraction_result` uses to
/// decide which renderer runs — not a looser proxy for it.
fn wants_dot_output(config: &ExtractionConfig) -> bool {
    matches!(&config.output_format, crate::core::config::OutputFormat::Custom(name) if name == "dot")
}

/// Mutable cursor state for walking a `quick_xml` event stream into an `InternalDocument`.
///
/// Each `on_*` method carries the body of one match arm from the original event loop in
/// `build_internal_document`, so element order, indices, and depth bookkeeping are unchanged.
struct XmlTreeBuilder {
    doc: InternalDocument,
    element_stack: Vec<String>,
    depth: u16,
    index: u32,
    is_svg: bool,
}

impl XmlTreeBuilder {
    fn new(doc: InternalDocument, is_svg: bool) -> Self {
        Self {
            doc,
            element_stack: Vec::new(),
            depth: 0,
            index: 0,
            is_svg,
        }
    }

    /// Collect an element's attributes, applying the security budget and trimming rules
    /// shared by `Start` and `Empty` events.
    fn collect_attributes(
        budget: &mut SecurityBudget,
        element: &quick_xml::events::BytesStart<'_>,
    ) -> Result<AHashMap<String, String>> {
        let mut attrs = AHashMap::new();
        for attr in element.attributes().flatten() {
            let key: std::borrow::Cow<str> = std::borrow::Cow::Borrowed(attr.key.as_ref());
            let val: std::borrow::Cow<str> = std::borrow::Cow::Borrowed(attr.value.as_ref());
            budget.check_attr(&key, &val)?;
            let trimmed_val = val.trim();
            if !trimmed_val.is_empty() {
                attrs.insert(key.to_string(), trimmed_val.to_string());
            }
        }
        Ok(attrs)
    }

    fn on_start(&mut self, e: &quick_xml::events::BytesStart<'_>, budget: &mut SecurityBudget) -> Result<()> {
        budget.enter()?;
        let name_owned = e.name().as_ref().to_string();
        let attrs = Self::collect_attributes(budget, e)?;

        let level = heading_level(self.depth);
        let mut elem =
            InternalElement::text(ElementKind::Heading { level }, &name_owned, self.depth).with_index(self.index);
        if !attrs.is_empty() {
            elem = elem.with_attributes(attrs);
        }
        self.doc.push_element(elem);
        self.index += 1;

        self.element_stack.push(name_owned);
        self.depth = self.depth.saturating_add(1);
        Ok(())
    }

    fn on_end(&mut self, budget: &mut SecurityBudget) {
        budget.leave();
        self.element_stack.pop();
        self.depth = self.depth.saturating_sub(1);
    }

    fn on_text(&mut self, e: &quick_xml::events::BytesText<'_>, budget: &mut SecurityBudget) -> Result<()> {
        if self.is_svg {
            let in_text_elem = self
                .element_stack
                .iter()
                .any(|n| matches!(n.as_str(), "text" | "tspan" | "title" | "desc" | "textPath"));
            if !in_text_elem {
                return Ok(());
            }
        }
        let text: std::borrow::Cow<str> = std::borrow::Cow::Borrowed(e.as_ref());
        budget.check_entity(&text)?;
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            budget.account_text(trimmed.len())?;
            let text_depth = if self.depth > 0 { self.depth - 1 } else { 0 };
            let elem = InternalElement::text(ElementKind::Paragraph, trimmed, text_depth).with_index(self.index);
            self.doc.push_element(elem);
            self.index += 1;
        }
        Ok(())
    }

    fn on_empty(&mut self, e: &quick_xml::events::BytesStart<'_>, budget: &mut SecurityBudget) -> Result<()> {
        let name = e.name().as_ref().to_string();
        let attrs = Self::collect_attributes(budget, e)?;

        let level = heading_level(self.depth);
        let mut elem = InternalElement::text(ElementKind::Heading { level }, &name, self.depth).with_index(self.index);
        if !attrs.is_empty() {
            elem = elem.with_attributes(attrs);
        }
        self.doc.push_element(elem);
        self.index += 1;
        Ok(())
    }

    fn on_cdata(&mut self, e: &quick_xml::events::BytesCData<'_>, budget: &mut SecurityBudget) -> Result<()> {
        let text: std::borrow::Cow<str> = std::borrow::Cow::Borrowed(e.as_ref());
        budget.check_entity(&text)?;
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            budget.account_text(trimmed.len())?;
            let elem = InternalElement::text(ElementKind::Paragraph, trimmed, self.depth).with_index(self.index);
            self.doc.push_element(elem);
            self.index += 1;
        }
        Ok(())
    }

    fn record_truncated_parse(&mut self, cause: &quick_xml::Error) {
        crate::core::diagnostics::push_truncated_parse_warning(
            &mut self.doc.processing_warnings,
            XML_WARNING_SOURCE,
            "the XML element tree",
            cause,
        );
    }

    fn finish(mut self) -> InternalDocument {
        // `check_end_names` is off, so a document cut off mid-tree never raises a parser
        // error — it just reaches EOF with elements still on the stack. Report that rather
        // than handing back a truncated tree that looks complete (#134).
        crate::core::diagnostics::push_unclosed_elements_warning(
            &mut self.doc.processing_warnings,
            XML_WARNING_SOURCE,
            &self.element_stack,
        );
        self.doc
    }
}

/// Build an `InternalDocument` from XML content by parsing element hierarchy.
///
/// Maps XML elements to headings (for parent elements with children) and
/// paragraphs (for text content), preserving the element tree structure.
/// Element attributes are stored as element attributes.
///
/// `budget` enforces hostile-input limits (XML depth, iteration count, entity
/// length, cumulative content size). Any limit violation is converted into a
/// `XbergError::Security` via the `?` operator at call-site.
fn build_internal_document(content: &[u8], mime_type: &str, budget: &mut SecurityBudget) -> Result<InternalDocument> {
    use crate::utils::xml_utils::EntityReader;
    use quick_xml::events::Event;

    let mut doc = InternalDocument::new("xml");
    let is_svg = mime_type == "image/svg+xml";

    let (decoded, decoded_lossily) = crate::utils::xml_utils::decode_xml_to_utf8(content);
    if decoded_lossily {
        crate::core::diagnostics::push_lossy_decode_warning(
            &mut doc.processing_warnings,
            XML_WARNING_SOURCE,
            "XML source",
        );
    }
    // No reader-level trim_text: EntityReader coalesces text fragments around
    // entity references, and trimming fragments first would corrupt spacing.
    // Text is trimmed below, after coalescing. ~keep
    let mut reader = EntityReader::from_bytes(decoded.as_bytes());
    reader.config_mut().check_end_names = false;

    let mut builder = XmlTreeBuilder::new(doc, is_svg);

    loop {
        budget.step()?;
        match reader.read_event() {
            Ok(Event::Start(e)) => builder.on_start(&e, budget)?,
            Ok(Event::End(_)) => builder.on_end(budget),
            Ok(Event::Text(e)) => builder.on_text(&e, budget)?,
            Ok(Event::Empty(e)) => builder.on_empty(&e, budget)?,
            Ok(Event::CData(e)) => builder.on_cdata(&e, budget)?,
            Ok(Event::Eof) => break,
            // A malformed event ends the parse; everything after it is lost, so
            // say so rather than returning a silently truncated document (#134).
            Err(e) => {
                builder.record_truncated_parse(&e);
                break;
            }
            _ => {}
        }
    }

    Ok(builder.finish())
}
#[cfg_attr(alef, alef(skip))]
/// XML extractor.
///
/// Extracts text content from XML files, preserving element structure information.
pub struct XmlExtractor;

impl XmlExtractor {
    /// Create a new XML extractor.
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Default for XmlExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for XmlExtractor {
    fn name(&self) -> &str {
        "xml-extractor"
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn initialize(&self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    fn description(&self) -> &str {
        "Extracts text content from XML files with element metadata"
    }

    fn author(&self) -> &str {
        "Xberg Team"
    }
}

impl SyncExtractor for XmlExtractor {
    fn extract_sync(&self, content: &[u8], mime_type: &str, config: &ExtractionConfig) -> Result<InternalDocument> {
        let default_limits;
        let limits: &crate::extractors::security::SecurityLimits = if let Some(ref l) = config.security_limits {
            l
        } else {
            default_limits = crate::extractors::security::SecurityLimits::default();
            &default_limits
        };
        let xml_result = if mime_type == "image/svg+xml" {
            parse_xml_svg(content, false, limits)?
        } else {
            parse_xml(content, false, limits)?
        };

        let mut budget = SecurityBudget::from_config(config);
        let mut doc = build_internal_document(content, mime_type, &mut budget)?;
        doc.mime_type = mime_type.to_string();

        // An SVG diagram carries its own node/edge structure, so it can be read
        // rather than inferred. Recovery reports `None` for drawings that are
        // not diagrams, which is most SVGs, and leaves the rest of extraction
        // untouched either way. Skipped outright unless the caller actually
        // asked for DOT output: the text pass alone can walk an unbounded
        // number of `<text>` elements, and every renderer but `dot` discards
        // the result anyway.
        #[cfg(feature = "svg")]
        if mime_type == "image/svg+xml"
            && wants_dot_output(config)
            && let Some(graph) = crate::extraction::diagram::svg::recover(content)
        {
            doc.diagrams.push(graph);
        }

        // A flat ODF drawing names its own shapes and connectors outright
        // (`draw:id`, `draw:start-shape`, `draw:end-shape`), so recovery here
        // is an exact lookup rather than the geometric matching SVG needs. ~keep
        if mime_type == ODG_FLAT_MIME_TYPE
            && wants_dot_output(config)
            && let Some(graph) = crate::extraction::diagram::odf::recover(content)
        {
            doc.diagrams.push(graph);
        }

        doc.metadata = Metadata {
            format: Some(crate::types::FormatMetadata::Xml(crate::types::XmlMetadata {
                element_count: xml_result.element_count as u32,
                unique_elements: xml_result.unique_elements,
            })),
            ..Default::default()
        };

        Ok(doc)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl InternalDocumentExtractor for XmlExtractor {
    async fn extract_content(
        &self,
        content: &[u8],
        mime_type: &str,
        config: &ExtractionConfig,
    ) -> Result<InternalDocument> {
        self.extract_sync(content, mime_type, config)
    }

    fn supported_mime_types(&self) -> &[&str] {
        &[
            "application/xml",
            "text/xml",
            KML_MIME_TYPE,
            "image/svg+xml",
            "application/x-endnote+xml",
            ODG_FLAT_MIME_TYPE,
        ]
    }

    fn priority(&self) -> i32 {
        50
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_xml(start_elements: usize, empty_leaf: bool) -> Vec<u8> {
        let mut xml = String::new();
        for _ in 0..start_elements {
            xml.push_str("<node>");
        }
        if empty_leaf {
            xml.push_str("<leaf/>");
        }
        for _ in 0..start_elements {
            xml.push_str("</node>");
        }
        xml.into_bytes()
    }

    fn assert_valid_heading_levels(doc: &InternalDocument) {
        for element in &doc.elements {
            let ElementKind::Heading { level } = element.kind else {
                continue;
            };
            assert!(
                (1..=MAX_XML_HEADING_LEVEL as u8).contains(&level),
                "heading level must stay in 1..=6, got {level} at depth {}",
                element.depth
            );
        }
    }

    #[test]
    fn deeply_nested_start_elements_clamp_heading_level_before_narrowing() {
        let content = nested_xml(256, false);
        let mut budget = SecurityBudget::with_defaults();

        let doc = build_internal_document(&content, "application/xml", &mut budget)
            .expect("depths within the default security limit must extract without overflow");

        assert_eq!(doc.elements.len(), 256);
        assert_eq!(doc.elements.last().map(|element| element.depth), Some(255));
        assert_eq!(
            doc.elements.last().map(|element| &element.kind),
            Some(&ElementKind::Heading { level: 6 })
        );
        assert_valid_heading_levels(&doc);
    }

    #[test]
    fn deeply_nested_empty_element_clamps_heading_level_before_narrowing() {
        let content = nested_xml(255, true);
        let mut budget = SecurityBudget::with_defaults();

        let doc = build_internal_document(&content, "application/xml", &mut budget)
            .expect("an empty element within the default security limit must not overflow");

        assert_eq!(doc.elements.len(), 256);
        assert_eq!(doc.elements.last().map(|element| element.depth), Some(255));
        assert_eq!(
            doc.elements.last().map(|element| &element.kind),
            Some(&ElementKind::Heading { level: 6 })
        );
        assert_valid_heading_levels(&doc);
    }

    #[test]
    fn deeply_nested_xml_still_respects_configured_security_limit() {
        let content = nested_xml(256, false);
        let limits = crate::extractors::security::SecurityLimits {
            max_xml_depth: 255,
            max_nesting_depth: 255,
            ..Default::default()
        };
        let mut budget = SecurityBudget::from_limits(&limits);

        let error = build_internal_document(&content, "application/xml", &mut budget)
            .expect_err("depth beyond the configured limit must be rejected");

        match error {
            crate::XbergError::Security { message, .. } => {
                assert_eq!(message, "Nesting too deep: 256 levels (max: 255)");
            }
            other => panic!("expected a security error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_xml_extractor() {
        let extractor = XmlExtractor::new();
        let content = b"<root><item>Hello</item><item>World</item></root>";
        let config = ExtractionConfig::default();

        let result = extractor
            .extract_content(content, "application/xml", &config)
            .await
            .unwrap();

        assert!(result.metadata.format.is_some());
        let xml_meta = match result.metadata.format.as_ref().unwrap() {
            crate::types::FormatMetadata::Xml(meta) => meta,
            _ => panic!("Expected Xml metadata"),
        };
        assert_eq!(xml_meta.element_count, 3);
        assert!(xml_meta.unique_elements.contains(&"root".to_string()));
        assert!(xml_meta.unique_elements.contains(&"item".to_string()));
    }

    #[tokio::test]
    async fn kml_uses_xml_extraction_and_preserves_its_mime_type() {
        let extractor = XmlExtractor::new();
        let content =
            br#"<kml xmlns="http://www.opengis.net/kml/2.2"><Placemark><name>Berlin</name></Placemark></kml>"#;

        assert!(
            extractor
                .supported_mime_types()
                .contains(&"application/vnd.google-earth.kml+xml")
        );
        let result = extractor
            .extract_content(
                content,
                "application/vnd.google-earth.kml+xml",
                &ExtractionConfig::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.mime_type, "application/vnd.google-earth.kml+xml");
        assert_eq!(
            crate::rendering::render_plain(&result),
            "kml\n  Placemark\n    name\n    Berlin"
        );
    }

    #[test]
    fn test_xml_plugin_interface() {
        let extractor = XmlExtractor::new();
        assert_eq!(extractor.name(), "xml-extractor");
        assert_eq!(extractor.version(), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            extractor.supported_mime_types(),
            &[
                "application/xml",
                "text/xml",
                KML_MIME_TYPE,
                "image/svg+xml",
                "application/x-endnote+xml",
                ODG_FLAT_MIME_TYPE
            ]
        );
        assert_eq!(extractor.priority(), 50);
    }

    /// Warnings emitted for a lossy decode specifically, identified by message content
    /// since `XML_WARNING_SOURCE` is also used for truncation and unclosed-element
    /// warnings.
    fn decode_warnings(doc: &InternalDocument) -> Vec<String> {
        doc.processing_warnings
            .iter()
            .filter(|w| w.source == XML_WARNING_SOURCE && w.message.contains("not valid UTF-8"))
            .map(|w| w.message.to_string())
            .collect()
    }

    /// #395: an explicit `<?xml encoding=...?>` declaration is the one place in this
    /// extractor where `replaced_characters` is reachable deterministically under
    /// *both* build configurations. The declared label is handed straight to
    /// `encoding_rs` and never passes through `quality`'s chardetng detection, so a
    /// declaration that does not match the actual bytes always decodes with errors --
    /// unlike the no-declaration path below, whose lossy outcome depends on detection.
    #[tokio::test]
    async fn should_warn_when_declared_encoding_does_not_match_the_bytes() {
        let extractor = XmlExtractor::new();
        let config = ExtractionConfig::default();
        let mut content = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><root>".to_vec();
        content.extend_from_slice(&[0xFF, 0xFE]);
        content.extend_from_slice(b"</root>");

        let result = extractor
            .extract_content(&content, "application/xml", &config)
            .await
            .expect("extraction of a mismatched declared encoding must still succeed");

        let warnings = decode_warnings(&result);
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one decode warning, got {warnings:?}"
        );
        assert!(
            warnings[0].contains("replacement character"),
            "warning must describe the lossy decode, got {warnings:?}"
        );
    }

    /// #395: with no `<?xml encoding=...?>` declaration, invalid UTF-8 bytes fall
    /// through to charset detection/lossy decoding and must still be reported.
    ///
    /// Deliberately not run under `quality`: there chardetng resolves these bytes to a
    /// single-byte encoding that maps all of 0x00-0xFF, so nothing is *replaced* -- see
    /// the identical note on `extractors::text::should_warn_when_text_source_is_not_valid_utf8`.
    #[cfg(not(feature = "quality"))]
    #[tokio::test]
    async fn should_warn_when_undeclared_source_is_not_valid_utf8() {
        let extractor = XmlExtractor::new();
        let config = ExtractionConfig::default();
        let mut content = b"<root>".to_vec();
        content.extend_from_slice(&[0xFF, 0xFE]);
        content.extend_from_slice(b"</root>");

        let result = extractor
            .extract_content(&content, "application/xml", &config)
            .await
            .expect("extraction of invalid UTF-8 must still succeed");

        let warnings = decode_warnings(&result);
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one decode warning, got {warnings:?}"
        );
        assert!(
            warnings[0].contains("replacement character"),
            "warning must describe the lossy decode, got {warnings:?}"
        );
    }

    /// A valid UTF-8 document -- declared or not -- must not produce a lossy-decode
    /// warning.
    #[tokio::test]
    async fn valid_utf8_xml_source_produces_zero_decode_warnings() {
        let extractor = XmlExtractor::new();
        let config = ExtractionConfig::default();
        let content = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><root>Hello</root>";

        let result = extractor
            .extract_content(content, "application/xml", &config)
            .await
            .expect("extraction should succeed");

        assert!(
            decode_warnings(&result).is_empty(),
            "valid UTF-8 must not warn about a lossy decode, got {:?}",
            decode_warnings(&result)
        );
    }
}
