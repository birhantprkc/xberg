package io.xberg.springai;

import io.xberg.ExtractInput;
import io.xberg.ExtractedDocument;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractionErrorItem;
import io.xberg.ExtractionResult;
import io.xberg.Xberg;
import io.xberg.XbergRsException;
import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;
import org.springframework.ai.document.Document;
import org.springframework.ai.document.DocumentReader;
import org.springframework.core.io.Resource;

/**
 * A Spring AI {@link DocumentReader} that uses Xberg for document extraction.
 *
 * <p>
 * Supports 107 document formats including PDF, DOCX, PPTX, images (with OCR),
 * and more. Each extracted document is split into Spring AI {@link Document}
 * instances using a priority-based strategy: chunks &gt; elements &gt; pages
 * &gt; whole document.
 *
 * <p>
 * A single resource is extracted through {@link Xberg#extract}; multiple
 * resources are extracted in one call through {@link Xberg#extractBatch}, which
 * is substantially faster than running a reader per resource.
 *
 * <p>
 * Use the {@link #builder()} to configure the reader:
 *
 * <pre>{@code
 * var reader = XbergDocumentReader.builder().resource(new
 * FileSystemResource("report.pdf")).build(); List<Document> docs =
 * reader.get();
 * }</pre>
 *
 * <p>
 * Read several files in one batch:
 *
 * <pre>{@code
 * var reader = XbergDocumentReader.builder()
 * 		.resources(List.of(new FileSystemResource("a.pdf"), new
 * FileSystemResource("b.docx"))).build(); List<Document> docs = reader.get();
 * }</pre>
 *
 * <p>
 * For a single non-file resource (e.g. {@code ByteArrayResource}), a MIME type
 * must be provided:
 *
 * <pre>{@code
 * var reader = XbergDocumentReader.builder().resource(new
 * ByteArrayResource(bytes)).mimeType("application/pdf") .build();
 * }</pre>
 */
public final class XbergDocumentReader implements DocumentReader {

  private final List<Resource> resources;
  private final ExtractionConfig extractionConfig;
  private final InputResolver inputResolver;
  private final DocumentMapper documentMapper;

  private XbergDocumentReader(Builder builder) {
    this.resources = List.copyOf(builder.resources);
    this.extractionConfig = builder.extractionConfig;
    this.inputResolver = new InputResolver(this.resources, builder.mimeType);
    this.documentMapper =
        new DocumentMapper(Map.copyOf(builder.additionalMetadata));
  }

  /**
   * Extracts the configured resources and returns a flat list of Spring AI
   * {@link Document} instances.
   *
   * <p>
   * Every extracted document is split following priority order: if it contains
   * chunks, each chunk becomes a document; otherwise elements are used, then
   * pages, and finally the whole content as a single document. Documents from
   * multiple resources are concatenated in resource order.
   *
   * @return list of documents with extracted text and metadata
   * @throws RuntimeException
   *             if extraction or I/O fails
   */
  @Override
  public List<Document> get() {
    try {
      List<ExtractInput> inputs = inputResolver.buildInputs();
      List<String> sources =
          resources.stream().map(inputResolver::resolveSource).toList();
      ExtractionResult result = runExtraction(inputs);
      checkErrors(result);
      List<ExtractedDocument> documents = result.results();
      if (documents == null || documents.isEmpty()) {
        throw new IllegalStateException("Xberg extraction returned no results");
      }
      List<Document> output = new ArrayList<>();
      for (int i = 0; i < documents.size(); i++) {
        String source = i < sources.size() ? sources.get(i)
                                           : sources.get(sources.size() - 1);
        output.addAll(documentMapper.mapToDocuments(documents.get(i), source));
      }
      return output;
    } catch (IOException e) {
      throw new RuntimeException("Failed to extract document", e);
    } catch (XbergRsException e) {
      throw new RuntimeException("Xberg extraction failed", e);
    }
  }

  /**
   * Returns a new {@link Builder} for constructing a
   * {@link XbergDocumentReader}.
   */
  public static Builder builder() { return new Builder(); }

  /**
   * Builder for {@link XbergDocumentReader}.
   *
   * <p>
   * At least one {@link Resource} must be provided. A single resource without a
   * filename (e.g. {@code ByteArrayResource}) also requires an explicit MIME
   * type. When reading multiple resources, each resource must carry a filename
   * so its MIME type can be resolved; per-resource MIME overrides are not
   * supported in batch mode.
   */
  public static final class Builder {

    private final List<Resource> resources = new ArrayList<>();
    private String mimeType;
    private ExtractionConfig extractionConfig;
    private final Map<String, Object> additionalMetadata =
        new LinkedHashMap<>();

    private Builder() {}

    /**
     * Adds a Spring {@link Resource} to extract text from. At least one is
     * required.
     */
    public Builder resource(Resource resource) {
      this.resources.add(resource);
      return this;
    }

    /**
     * Adds all of the given resources for batch extraction via {@link
     * Xberg#extractBatch}.
     */
    public Builder resources(List<Resource> resources) {
      this.resources.addAll(resources);
      return this;
    }

    /**
     * Sets an explicit MIME type for the resource. Required when a single
     * resource has no filename (e.g. {@code ByteArrayResource}). Overrides any
     * MIME type guessed from the filename. Only valid when exactly one resource
     * is configured.
     */
    public Builder mimeType(String mimeType) {
      this.mimeType = mimeType;
      return this;
    }

    /**
     * Sets the Xberg {@link ExtractionConfig} to control extraction behavior
     * (chunking, OCR, keywords, NER, and every other capability the engine
     * exposes).
     */
    public Builder extractionConfig(ExtractionConfig config) {
      this.extractionConfig = config;
      return this;
    }

    /**
     * Adds all entries from the given map as additional metadata on each output
     * document.
     */
    public Builder metadata(Map<String, Object> metadata) {
      this.additionalMetadata.putAll(metadata);
      return this;
    }

    /**
     * Adds a single key-value pair as additional metadata on each output
     * document.
     */
    public Builder metadata(String key, Object value) {
      this.additionalMetadata.put(key, value);
      return this;
    }

    /**
     * Builds the reader, validating that required fields are set.
     *
     * @throws IllegalArgumentException
     *             if no resource is set, if a lone resource has neither a
     * filename nor a MIME type, or if a batch resource has no filename
     */
    public XbergDocumentReader build() {
      if (resources.isEmpty()) {
        throw new IllegalArgumentException("at least one resource is required");
      }
      if (resources.size() == 1) {
        validateSingle(resources.get(0));
      } else {
        validateBatch();
      }
      return new XbergDocumentReader(this);
    }

    private void validateSingle(Resource resource) {
      if (resource.getFilename() == null && mimeType == null) {
        throw new IllegalArgumentException(
            "mimeType is required when resource has no filename (e.g. "
            + "ByteArrayResource)");
      }
    }

    private void validateBatch() {
      if (mimeType != null) {
        throw new IllegalArgumentException(
            "mimeType is only supported when reading a single resource");
      }
      for (Resource resource : resources) {
        if (resource.getFilename() == null) {
          throw new IllegalArgumentException(
              "each resource must have a filename when reading multiple "
              + "resources");
        }
      }
    }
  }

  /**
   * Runs {@link Xberg#extract} for a single input and {@link
   * Xberg#extractBatch} for multiple inputs. Batch extraction shares the
   * engine's work across inputs and is markedly faster than one reader per
   * resource.
   */
  private ExtractionResult runExtraction(List<ExtractInput> inputs)
      throws XbergRsException {
    ExtractionConfig config = extractionConfig != null
                                  ? extractionConfig
                                  : ExtractionConfig.builder().build();
    if (inputs.size() == 1) {
      return Xberg.extract(inputs.get(0), config);
    }
    return Xberg.extractBatch(inputs, config);
  }

  /**
   * Fails loudly if the engine reported any per-input error, surfacing the
   * offending source and message rather than silently dropping a resource.
   */
  private static void checkErrors(ExtractionResult result) {
    List<ExtractionErrorItem> errors = result.errors();
    if (errors == null || errors.isEmpty()) {
      return;
    }
    String detail = errors.stream()
                        .map(error -> error.source() + ": " + error.message())
                        .collect(Collectors.joining("; "));
    throw new IllegalStateException("Xberg extraction reported errors: " +
                                    detail);
  }
}
