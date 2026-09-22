package io.xberg.springai;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;

import io.xberg.ExtractInput;
import io.xberg.ExtractionConfig;
import io.xberg.Xberg;
import io.xberg.XbergRsException;
import java.io.IOException;
import java.util.List;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.mockito.MockedStatic;
import org.mockito.Mockito;
import org.springframework.core.io.ByteArrayResource;
import org.springframework.core.io.FileSystemResource;
import org.springframework.core.io.Resource;

class XbergDocumentReaderValidationTest {

  @Nested
  class BuilderValidation {

    @Test
    void shouldThrowWhenResourceIsNull() {
      assertThatThrownBy(() -> XbergDocumentReader.builder().build())
          .isInstanceOf(IllegalArgumentException.class)
          .hasMessageContaining("resource is required");
    }

    @Test
    void shouldThrowWhenByteArrayResourceWithoutMimeType() {
      ByteArrayResource resource = new ByteArrayResource(new byte[] {1, 2, 3});
      assertThatThrownBy(
          () -> XbergDocumentReader.builder().resource(resource).build())
          .isInstanceOf(IllegalArgumentException.class)
          .hasMessageContaining("mimeType is required");
    }

    @Test
    void shouldBuildWithFileSystemResource() {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      XbergDocumentReader reader =
          XbergDocumentReader.builder().resource(resource).build();
      assertThat(reader).isNotNull();
    }

    @Test
    void shouldBuildWithByteArrayResourceAndMimeType() {
      ByteArrayResource resource = new ByteArrayResource(new byte[] {1, 2, 3});
      XbergDocumentReader reader = XbergDocumentReader.builder()
                                       .resource(resource)
                                       .mimeType("application/pdf")
                                       .build();
      assertThat(reader).isNotNull();
    }

    @Test
    void shouldThrowWhenNoResource() {
      assertThatThrownBy(()
                             -> XbergDocumentReader.builder()
                                    .mimeType("application/pdf")
                                    .build())
          .isInstanceOf(IllegalArgumentException.class)
          .hasMessageContaining("at least one resource");
    }

    @Test
    void shouldRejectMimeTypeWithMultipleResources() {
      FileSystemResource first =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      FileSystemResource second =
          new FileSystemResource("src/test/resources/fixtures/sample.docx");
      assertThatThrownBy(()
                             -> XbergDocumentReader.builder()
                                    .resources(List.of(first, second))
                                    .mimeType("application/pdf")
                                    .build())
          .isInstanceOf(IllegalArgumentException.class)
          .hasMessageContaining("single resource");
    }

    @Test
    void shouldThrowWhenBatchResourceMissingFilename() {
      FileSystemResource named =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");
      ByteArrayResource unnamed = new ByteArrayResource(new byte[] {1, 2, 3});
      assertThatThrownBy(()
                             -> XbergDocumentReader.builder()
                                    .resources(List.of(named, unnamed))
                                    .build())
          .isInstanceOf(IllegalArgumentException.class)
          .hasMessageContaining("must have a filename");
    }
  }

  @Nested
  class ErrorHandling {

    @Test
    void shouldWrapXbergRsException() {
      FileSystemResource resource =
          new FileSystemResource("src/test/resources/fixtures/sample.pdf");

      try (MockedStatic<Xberg> mocked = Mockito.mockStatic(Xberg.class)) {
        mocked
            .when(()
                      -> Xberg.extract(any(ExtractInput.class),
                                       any(ExtractionConfig.class)))
            .thenThrow(new XbergRsException(1000, "extraction failed"));

        XbergDocumentReader reader =
            XbergDocumentReader.builder().resource(resource).build();

        assertThatThrownBy(reader::get)
            .isInstanceOf(RuntimeException.class)
            .hasMessageContaining("Xberg extraction failed")
            .hasCauseInstanceOf(XbergRsException.class);
      }
    }

    @Test
    void shouldWrapIOException() throws Exception {
      Resource mockResource = Mockito.mock(Resource.class);
      Mockito.when(mockResource.getFilename()).thenReturn("test.pdf");
      Mockito.when(mockResource.getInputStream())
          .thenThrow(new IOException("read failed"));

      XbergDocumentReader reader =
          XbergDocumentReader.builder().resource(mockResource).build();

      assertThatThrownBy(reader::get)
          .isInstanceOf(RuntimeException.class)
          .hasMessageContaining("Failed to extract document")
          .hasCauseInstanceOf(IOException.class);
    }
  }
}
