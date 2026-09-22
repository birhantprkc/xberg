package io.xberg.springai;

import static io.xberg.springai.XbergDocumentReaderTestFixtures.createResult;
import static io.xberg.springai.XbergDocumentReaderTestFixtures.mockExtract;
import static org.assertj.core.api.Assertions.assertThat;

import io.xberg.ExtractionResult;
import io.xberg.Xberg;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.mockito.MockedStatic;
import org.mockito.Mockito;
import org.springframework.ai.document.Document;
import org.springframework.core.io.FileSystemResource;

class XbergDocumentReaderMetadataTest {

  @Nested
  class BaseMetadataMapping {

    @Test
    void shouldMapAllExplicitMetadataFields() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {
					    "title": "Test Document",
					    "subject": "Testing",
					    "authors": ["John Doe", "Jane Smith"],
					    "keywords": ["test", "document"],
					    "language": "en",
					    "created_at": "2025-01-01T00:00:00Z",
					    "modified_at": "2025-06-01T00:00:00Z",
					    "created_by": "TestUser",
					    "modified_by": "TestEditor",
					    "category": "Testing",
					    "tags": ["unit-test", "integration"],
					    "document_version": "1.0",
					    "abstract_text": "A test document abstract",
					    "output_format": "plain"
					  },
					  "tables": [{"cells": [["A","B"]], "markdown": "| A | B |", "page_number": 0}],
					  "detected_languages": ["en", "de"],
					  "quality_score": 0.95,
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": [],
					  "extracted_keywords": [{"text": "test", "score": 0.9, "algorithm": "yake"}],
					  "processing_warnings": [{"source": "ocr", "message": "Low confidence"}]
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("source")).isEqualTo("sample.pdf");
        assertThat(metadata.get("mime_type")).isEqualTo("application/pdf");
        assertThat(metadata.get("title")).isEqualTo("Test Document");
        assertThat(metadata.get("subject")).isEqualTo("Testing");
        assertThat(metadata.get("authors")).isEqualTo("John Doe, Jane Smith");
        assertThat(metadata.get("keywords")).isEqualTo("test, document");
        assertThat(metadata.get("language")).isEqualTo("en");
        assertThat(metadata.get("created_at"))
            .isEqualTo("2025-01-01T00:00:00Z");
        assertThat(metadata.get("modified_at"))
            .isEqualTo("2025-06-01T00:00:00Z");
        assertThat(metadata.get("created_by")).isEqualTo("TestUser");
        assertThat(metadata.get("modified_by")).isEqualTo("TestEditor");
        assertThat(metadata.get("category")).isEqualTo("Testing");
        assertThat(metadata.get("tags")).isEqualTo("unit-test, integration");
        assertThat(metadata.get("document_version")).isEqualTo("1.0");
        assertThat(metadata.get("abstract_text"))
            .isEqualTo("A test document abstract");
        assertThat(metadata.get("output_format")).isEqualTo("plain");
        assertThat(metadata.get("detected_languages")).isEqualTo("en, de");
        assertThat(metadata.get("quality_score")).isEqualTo(0.95);
        assertThat(metadata.get("table_count")).isEqualTo(1);
        assertThat(metadata).containsKey("tables");
        assertThat(metadata).containsKey("extracted_keywords");
        assertThat(metadata).containsKey("processing_warnings");
      }
    }

    @Test
    void shouldMapMinimalMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"Minimal","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":["en"],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata).containsKey("source");
        assertThat(metadata).containsKey("mime_type");
        assertThat(metadata).containsKey("page_count");
        assertThat(metadata).containsKey("detected_languages");
        assertThat(metadata).containsKey("table_count");
        assertThat(metadata).doesNotContainKey("title");
        assertThat(metadata).doesNotContainKey("subject");
        assertThat(metadata).doesNotContainKey("authors");
      }
    }

    @Test
    void shouldJoinListMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {
					    "authors": ["John", "Jane"],
					    "keywords": ["a", "b", "c"],
					    "tags": ["x", "y"]
					  },
					  "tables": [],
					  "detected_languages": ["en", "de"],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("authors")).isEqualTo("John, Jane");
        assertThat(metadata.get("keywords")).isEqualTo("a, b, c");
        assertThat(metadata.get("tags")).isEqualTo("x, y");
        assertThat(metadata.get("detected_languages")).isEqualTo("en, de");
      }
    }

    @Test
    void shouldMergeUserMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {"title": "Original Title"},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resource(resource)
                                  .metadata("title", "User Title")
                                  .metadata("custom_key", "custom_value")
                                  .build()
                                  .get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("title")).isEqualTo("User Title");
        assertThat(metadata.get("custom_key")).isEqualTo("custom_value");
      }
    }
  }

  @Nested
  class MetadataSerialization {

    @Test
    void shouldSerializeTablesToJson() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [{"cells": [["A","B"]], "markdown": "| A | B |", "page_number": 0}],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("table_count")).isEqualTo(1);
        assertThat(metadata.get("tables")).isInstanceOf(String.class);
        String tablesJson = (String)metadata.get("tables");
        assertThat(tablesJson).contains("| A | B |");
      }
    }

    @Test
    void shouldPassThroughFormatSpecificMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {
					    "additional": {
					      "pdf_version": "1.7",
					      "producer": "TestProducer",
					      "custom_list": ["a", "b"]
					    }
					  },
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("pdf_version")).isEqualTo("1.7");
        assertThat(metadata.get("producer")).isEqualTo("TestProducer");
        assertThat(metadata.get("custom_list")).isInstanceOf(String.class);
      }
    }

    @Test
    void shouldLetExplicitFieldsOverrideAdditional() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Text",
					  "mime_type": "application/pdf",
					  "metadata": {
					    "title": "Typed Title",
					    "subject": "Typed Subject"
					  },
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("title")).isEqualTo("Typed Title");
        assertThat(metadata.get("subject")).isEqualTo("Typed Subject");
      }
    }
  }
}
