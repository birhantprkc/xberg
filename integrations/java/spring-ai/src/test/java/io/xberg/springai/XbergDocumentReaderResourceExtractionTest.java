package io.xberg.springai;

import static io.xberg.springai.XbergDocumentReaderTestFixtures.createResult;
import static io.xberg.springai.XbergDocumentReaderTestFixtures.mockExtract;
import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;

import io.xberg.ExtractInput;
import io.xberg.ExtractInputKind;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractionResult;
import io.xberg.Xberg;
import java.util.List;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.mockito.MockedStatic;
import org.mockito.Mockito;
import org.springframework.ai.document.Document;
import org.springframework.core.io.ByteArrayResource;
import org.springframework.core.io.ClassPathResource;
import org.springframework.core.io.FileSystemResource;

class XbergDocumentReaderResourceExtractionTest {

  @Nested
  class FileSystemResourceExtraction {

    @Test
    void shouldExtractFromFileSystemResourceUsingUriInput() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"Hello","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":["en"],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("Hello");

        ArgumentCaptor<ExtractInput> captor =
            ArgumentCaptor.forClass(ExtractInput.class);
        mocked.verify(
            () -> Xberg.extract(captor.capture(), any(ExtractionConfig.class)));
        ExtractInput input = captor.getValue();
        assertThat(input.kind()).isEqualTo(ExtractInputKind.URI);
        assertThat(input.uri()).endsWith("sample.pdf");
        assertThat(input.filename()).isEqualTo("sample.pdf");
      }
    }

    @Test
    void shouldPassExtractionConfigToExtract() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionConfig config = ExtractionConfig.builder().build();
      ExtractionResult result = createResult("""
					{"content":"Hello","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resource(resource)
                                  .extractionConfig(config)
                                  .build()
                                  .get();

        assertThat(docs).hasSize(1);

        ArgumentCaptor<ExtractionConfig> captor =
            ArgumentCaptor.forClass(ExtractionConfig.class);
        mocked.verify(
            () -> Xberg.extract(any(ExtractInput.class), captor.capture()));
        assertThat(captor.getValue()).isSameAs(config);
      }
    }
  }

  @Nested
  class ByteArrayResourceExtraction {

    @Test
    void shouldExtractFromByteArrayResourceUsingBytesInput() throws Exception {
      byte[] data = new byte[] {1, 2, 3};
      ByteArrayResource resource = new ByteArrayResource(data);
      ExtractionResult result = createResult("""
					{"content":"Bytes content","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resource(resource)
                                  .mimeType("application/pdf")
                                  .build()
                                  .get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("Bytes content");

        ArgumentCaptor<ExtractInput> captor =
            ArgumentCaptor.forClass(ExtractInput.class);
        mocked.verify(
            () -> Xberg.extract(captor.capture(), any(ExtractionConfig.class)));
        ExtractInput input = captor.getValue();
        assertThat(input.kind()).isEqualTo(ExtractInputKind.BYTES);
        assertThat(input.mimeType()).isEqualTo("application/pdf");
        assertThat(input.bytes()).isEqualTo(data);
      }
    }

    @Test
    void shouldPassMimeTypeToBytesInput() throws Exception {
      byte[] data = new byte[] {4, 5, 6};
      ByteArrayResource resource = new ByteArrayResource(data);
      ExtractionResult result = createResult("""
							{"content":"DOCX content","mime_type":"application/vnd.openxmlformats-officedocument.wordprocessingml.document",\
							"metadata":{},"tables":[],"detected_languages":[],"chunks":[],"images":[],\
							"pages":[],"elements":[]}""");

      String docxMime =
          "application/"
          + "vnd.openxmlformats-officedocument.wordprocessingml.document";

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        XbergDocumentReader.builder()
            .resource(resource)
            .mimeType(docxMime)
            .build()
            .get();

        ArgumentCaptor<ExtractInput> captor =
            ArgumentCaptor.forClass(ExtractInput.class);
        mocked.verify(
            () -> Xberg.extract(captor.capture(), any(ExtractionConfig.class)));
        assertThat(captor.getValue().mimeType()).isEqualTo(docxMime);
      }
    }
  }

  @Nested
  class ClassPathResourceExtraction {

    @Test
    void shouldExtractFromClassPathResource() throws Exception {
      ClassPathResource resource = new ClassPathResource("fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"ClassPath content","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs).hasSize(1);
        assertThat(docs.getFirst().getText()).isEqualTo("ClassPath content");

        ArgumentCaptor<ExtractInput> captor =
            ArgumentCaptor.forClass(ExtractInput.class);
        mocked.verify(
            () -> Xberg.extract(captor.capture(), any(ExtractionConfig.class)));
        assertThat(captor.getValue().kind()).isEqualTo(ExtractInputKind.BYTES);
      }
    }

    @Test
    void shouldUseExplicitMimeTypeOverFilename() throws Exception {
      ClassPathResource resource = new ClassPathResource("fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"Override content","mime_type":"text/plain","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        XbergDocumentReader.builder()
            .resource(resource)
            .mimeType("text/plain")
            .build()
            .get();

        ArgumentCaptor<ExtractInput> captor =
            ArgumentCaptor.forClass(ExtractInput.class);
        mocked.verify(
            () -> Xberg.extract(captor.capture(), any(ExtractionConfig.class)));
        assertThat(captor.getValue().mimeType()).isEqualTo("text/plain");
      }
    }
  }

  @Nested
  class SourceResolution {

    @Test
    void shouldResolveSourceFromFilename() throws Exception {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ExtractionResult result = createResult("""
					{"content":"Text","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs =
            XbergDocumentReader.builder().resource(resource).build().get();

        assertThat(docs.getFirst().getMetadata().get("source"))
            .isEqualTo("sample.pdf");
      }
    }

    @Test
    void shouldResolveSourceForByteArrayResource() throws Exception {
      ByteArrayResource resource = new ByteArrayResource(new byte[] {1, 2, 3});
      ExtractionResult result = createResult("""
					{"content":"Text","mime_type":"application/pdf","metadata":{},\
					"tables":[],"detected_languages":[],"chunks":[],"images":[],\
					"pages":[],"elements":[]}""");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mockExtract(mocked, result);

        List<Document> docs = XbergDocumentReader.builder()
                                  .resource(resource)
                                  .mimeType("application/pdf")
                                  .build()
                                  .get();

        assertThat(docs.getFirst().getMetadata().get("source"))
            .isEqualTo("bytes://application/pdf");
      }
    }
  }
}
