package io.xberg.springai;

import static io.xberg.springai.XbergDocumentReaderTestFixtures.createResult;
import static io.xberg.springai.XbergDocumentReaderTestFixtures.mockExtract;
import static io.xberg.springai.XbergDocumentReaderTestFixtures.mockExtractBatch;
import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;

import io.xberg.ExtractInput;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractionErrorItem;
import io.xberg.ExtractionResult;
import io.xberg.ExtractionResultFactory;
import io.xberg.Xberg;
import java.util.List;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.mockito.MockedStatic;
import org.mockito.Mockito;
import org.springframework.ai.document.Document;
import org.springframework.core.io.FileSystemResource;

class XbergDocumentReaderBatchAndPriorityTest {

  @Nested
  class BatchExtraction {

    private static final String DOC_A = """
				{"content":"Doc A","mime_type":"application/pdf","metadata":{},\
				"tables":[],"detected_languages":[],"chunks":[],"images":[],\
				"pages":[],"elements":[]}""";

    private static final String DOC_B = """
				{"content":"Doc B","mime_type":\
				"application/vnd.openxmlformats-officedocument.wordprocessingml.document","metadata":{},\
				"tables":[],"detected_languages":[],"chunks":[],"images":[],\
				"pages":[],"elements":[]}""";

    @Test
    void shouldExtractMultipleResourcesViaExtractBatch() throws Exception {
      FileSystemResource first =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      FileSystemResource second =
          new FileSystemResource("src/test/resources/fixtures/sample.docx");
      ExtractionResult result =
          ExtractionResultFactory.fromDocuments(List.of(DOC_A, DOC_B));

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtractBatch(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resources(List.of(first, second))
                                  .build()
                                  .get();

        assertThat(docs).hasSize(2);
        assertThat(docs.get(0).getText()).isEqualTo("Doc A");
        assertThat(docs.get(1).getText()).isEqualTo("Doc B");

        ArgumentCaptor<List<ExtractInput>> captor =
            ArgumentCaptor.forClass(List.class);
        mocked.verify(()
                          -> Xberg.extractBatch(captor.capture(),
                                                any(ExtractionConfig.class)));
        assertThat(captor.getValue()).hasSize(2);
        mocked.verify(()
                          -> Xberg.extract(any(ExtractInput.class),
                                           any(ExtractionConfig.class)),
                      Mockito.never());
      }
    }

    @Test
    void shouldMapSourcePerDocument() throws Exception {
      FileSystemResource first =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      FileSystemResource second =
          new FileSystemResource("src/test/resources/fixtures/sample.docx");
      ExtractionResult result =
          ExtractionResultFactory.fromDocuments(List.of(DOC_A, DOC_B));

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtractBatch(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resources(List.of(first, second))
                                  .build()
                                  .get();

        assertThat(docs.get(0).getMetadata().get("source"))
            .isEqualTo("sample.pdf");
        assertThat(docs.get(1).getMetadata().get("source"))
            .isEqualTo("sample.docx");
      }
    }

    @Test
    void shouldThrowWhenBatchReportsErrors() throws Exception {
      FileSystemResource first =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      FileSystemResource second =
          new FileSystemResource("src/test/resources/fixtures/sample.docx");
      ExtractionErrorItem error = ExtractionErrorItem.builder()
                                      .withIndex(1)
                                      .withCode(1000)
                                      .withErrorType("Extraction")
                                      .withSource("sample.docx")
                                      .withMessage("boom")
                                      .build();
      ExtractionResult result =
          ExtractionResultFactory.withErrors(List.of(DOC_A), List.of(error));

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtractBatch(mocked, result);

        XbergDocumentReader reader = XbergDocumentReader.builder()
                                         .resources(List.of(first, second))
                                         .build();

        assertThatThrownBy(reader::get)
            .isInstanceOf(IllegalStateException.class)
            .hasMessageContaining("sample.docx")
            .hasMessageContaining("boom");
      }
    }
  }

  @Nested
  class SplittingPriority {

    @Test
    void shouldPreferChunksOverPages() throws Exception {
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
					      "content": "Chunk content",
					      "metadata": {"chunk_index": 0, "total_chunks": 1, "byte_start": 0, "byte_end": 50}
					    }
					  ],
					  "images": [],
					  "pages": [
					    {"page_number": 1, "content": "Page content", "tables": [], "images": []}
					  ],
					  "elements": []
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("Chunk content");
      }
    }

    @Test
    void shouldPreferElementsOverPages() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{
					  "content": "Full text",
					  "mime_type": "application/pdf",
					  "metadata": {},
					  "tables": [],
					  "detected_languages": [],
					  "chunks": [],
					  "images": [],
					  "pages": [
					    {"page_number": 1, "content": "Page content", "tables": [], "images": []}
					  ],
					  "elements": [
					    {
					      "element_type": "narrative_text",
					      "text": "Element content",
					      "metadata": {"page_number": 1}
					    }
					  ]
					}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("Element content");
      }
    }

    @Test
    void shouldReturnSingleDocumentWhenNothingPresent() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"Single content","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("Single content");
      }
    }
  }
}
