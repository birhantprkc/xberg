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

class XbergDocumentReaderSplittingTest {

  @Nested
  class ChunkBasedSplitting {

    @Test
    void shouldCreateDocumentsFromChunks() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Full text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [
					    {
					      "content": "Chunk 1 text",
					      "metadata": {"chunk_index": 0, "total_chunks": 2, "byte_start": 0, "byte_end": 100}
					    },
					    {
					      "content": "Chunk 2 text",
					      "metadata": {"chunk_index": 1, "total_chunks": 2, "byte_start": 100, "byte_end": 200}
					    }
					  ],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(2);
        assertThat(docs.get(0).getText()).isEqualTo("Chunk 1 text");
        assertThat(docs.get(1).getText()).isEqualTo("Chunk 2 text");
      }
    }

    @Test
    void shouldPopulateChunkMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Full text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [
					    {
					      "content": "Chunk text",
					      "metadata": {
					        "chunk_index": 0,
					        "total_chunks": 1,
					        "byte_start": 0,
					        "byte_end": 50,
					        "token_count": 10,
					        "first_page": 1,
					        "last_page": 2,
					        "heading_context": {
					          "headings": [{"level": 1, "text": "Introduction"}]
					        }
					      }
					    }
					  ],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("chunk_index")).isEqualTo(0L);
        assertThat(metadata.get("total_chunks")).isEqualTo(1L);
        assertThat(metadata.get("token_count")).isEqualTo(10L);
        assertThat(metadata.get("first_page")).isEqualTo(1);
        assertThat(metadata.get("last_page")).isEqualTo(2);
        assertThat(metadata.get("heading_context")).isInstanceOf(String.class);
        assertThat((String)metadata.get("heading_context"))
            .contains("Introduction");
      }
    }

    @Test
    void shouldJoinHeadingPath() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Full text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [
					    {
					      "content": "Chunk text",
					      "chunk_type": "heading",
					      "metadata": {
					        "chunk_index": 0,
					        "total_chunks": 1,
					        "byte_start": 0,
					        "byte_end": 50,
					        "heading_path": ["Chapter 1", "Section 1.2"]
					      }
					    }
					  ],
					  "images": [],
					  "pages": [],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("heading_path"))
            .isEqualTo("Chapter 1 > Section 1.2");
        assertThat(metadata.get("chunk_type")).isEqualTo("heading");
      }
    }
  }

  @Nested
  class ElementBasedSplitting {

    @Test
    void shouldCreateDocumentsFromElements() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Title text\\nParagraph text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": [
					    {
					      "element_type": "title",
					      "text": "Title text",
					      "metadata": {"page_number": 1, "element_index": 0}
					    },
					    {
					      "element_type": "narrative_text",
					      "text": "Paragraph text",
					      "metadata": {"page_number": 1, "element_index": 1}
					    }
					  ]
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(2);
        assertThat(docs.get(0).getText()).isEqualTo("Title text");
        assertThat(docs.get(1).getText()).isEqualTo("Paragraph text");
      }
    }

    @Test
    void shouldPopulateElementMetadata() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Title text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [],
					  "elements": [
					    {
					      "element_type": "title",
					      "text": "Title text",
					      "metadata": {
					        "page_number": 1,
					        "element_index": 0,
					        "coordinates": {"x0": 10.0, "y0": 20.0, "x1": 500.0, "y1": 50.0}
					      }
					    }
					  ]
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        Map<String, Object> metadata = docs.getFirst().getMetadata();
        assertThat(metadata.get("element_type")).isEqualTo("title");
        assertThat(metadata.get("page_number")).isEqualTo(1);
        assertThat(metadata.get("element_index")).isEqualTo(0L);
        assertThat(metadata.get("bbox_x0")).isEqualTo(10.0);
        assertThat(metadata.get("bbox_y0")).isEqualTo(20.0);
        assertThat(metadata.get("bbox_x1")).isEqualTo(500.0);
        assertThat(metadata.get("bbox_y1")).isEqualTo(50.0);
      }
    }
  }

  @Nested
  class PageSplitting {

    @Test
    void shouldCreateDocumentsFromPages() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Page 1 text\\nPage 2 text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": ["en"],
					  "chunks": [],
					  "images": [],
					  "pages": [
					    {"page_number": 1, "content": "Page 1 text", "tables": [], "images": []},
					    {"page_number": 2, "content": "Page 2 text", "tables": [], "images": []}
					  ],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(2);
        assertThat(docs.get(0).getText()).isEqualTo("Page 1 text");
        assertThat(docs.get(1).getText()).isEqualTo("Page 2 text");
        assertThat(docs.get(0).getMetadata().get("page")).isEqualTo(1);
        assertThat(docs.get(1).getMetadata().get("page")).isEqualTo(2);
      }
    }
  }
}
