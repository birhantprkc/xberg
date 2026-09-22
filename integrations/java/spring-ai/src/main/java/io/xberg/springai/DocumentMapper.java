package io.xberg.springai;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import io.xberg.BoundingBox;
import io.xberg.Chunk;
import io.xberg.ChunkMetadata;
import io.xberg.Element;
import io.xberg.ElementMetadata;
import io.xberg.ExtractedDocument;
import io.xberg.Metadata;
import io.xberg.PageContent;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.springframework.ai.document.Document;

/**
 * Maps an Xberg {@link ExtractedDocument} to Spring AI {@link Document}
 * instances and builds their metadata.
 *
 * <p>
 * Split out of {@link XbergDocumentReader} to keep that class within the repo's
 * type-length budget; holds only the user-supplied additional metadata from
 * {@link XbergDocumentReader.Builder#metadata}. ~keep
 */
final class DocumentMapper {

  private static final ObjectMapper OBJECT_MAPPER = new ObjectMapper();

  private final Map<String, Object> additionalMetadata;

  DocumentMapper(Map<String, Object> additionalMetadata) {
    this.additionalMetadata = additionalMetadata;
  }

  /**
   * Maps an extracted document to Spring AI documents using the
   * highest-granularity splitting available: chunks &gt; elements &gt; pages
   * &gt; whole document.
   */
  List<Document> mapToDocuments(ExtractedDocument document, String source) {
    Map<String, Object> baseMetadata = buildBaseMetadata(document, source);

    List<Chunk> chunks = document.chunks();
    if (chunks != null && !chunks.isEmpty()) {
      return mapChunksToDocuments(chunks, baseMetadata);
    }

    List<Element> elements = document.elements();
    if (elements != null && !elements.isEmpty()) {
      return mapElementsToDocuments(elements, baseMetadata);
    }

    List<PageContent> pages = document.pages();
    if (pages != null && !pages.isEmpty()) {
      return mapPagesToDocuments(pages, baseMetadata);
    }

    return List.of(new Document(document.content(), baseMetadata));
  }

  private List<Document>
  mapChunksToDocuments(List<Chunk> chunks, Map<String, Object> baseMetadata) {
    return chunks.stream()
        .map(chunk -> {
          Map<String, Object> metadata = new LinkedHashMap<>(baseMetadata);
          ChunkMetadata chunkMeta = chunk.metadata();
          metadata.put("chunk_index", chunkMeta.chunkIndex());
          metadata.put("total_chunks", chunkMeta.totalChunks());
          if (chunk.chunkType() != null) {
            metadata.put("chunk_type", chunk.chunkType().getValue());
          }
          if (chunkMeta.tokenCount() != null) {
            metadata.put("token_count", chunkMeta.tokenCount());
          }
          if (chunkMeta.firstPage() != null) {
            metadata.put("first_page", chunkMeta.firstPage());
          }
          if (chunkMeta.lastPage() != null) {
            metadata.put("last_page", chunkMeta.lastPage());
          }
          if (chunkMeta.headingPath() != null &&
              !chunkMeta.headingPath().isEmpty()) {
            metadata.put("heading_path",
                         String.join(" > ", chunkMeta.headingPath()));
          }
          if (chunkMeta.headingContext() != null) {
            metadata.put("heading_context", toJson(chunkMeta.headingContext()));
          }
          return new Document(chunk.content(), metadata);
        })
        .toList();
  }

  private List<Document>
  mapElementsToDocuments(List<Element> elements,
                         Map<String, Object> baseMetadata) {
    return elements.stream()
        .map(element -> {
          Map<String, Object> metadata = new LinkedHashMap<>(baseMetadata);
          metadata.put("element_type", element.elementType().getValue());
          ElementMetadata elemMeta = element.metadata();
          if (elemMeta.elementIndex() != null) {
            metadata.put("element_index", elemMeta.elementIndex());
          }
          if (elemMeta.pageNumber() != null) {
            metadata.put("page_number", elemMeta.pageNumber());
          }
          BoundingBox bbox = elemMeta.coordinates();
          if (bbox != null) {
            metadata.put("bbox_x0", bbox.x0());
            metadata.put("bbox_y0", bbox.y0());
            metadata.put("bbox_x1", bbox.x1());
            metadata.put("bbox_y1", bbox.y1());
          }
          return new Document(element.text(), metadata);
        })
        .toList();
  }

  private List<Document> mapPagesToDocuments(List<PageContent> pages,
                                             Map<String, Object> baseMetadata) {
    return pages.stream()
        .map(page -> {
          Map<String, Object> metadata = new LinkedHashMap<>(baseMetadata);
          metadata.put("page", page.pageNumber());
          return new Document(page.content(), metadata);
        })
        .toList();
  }

  /**
   * Builds the base metadata map applied to every output document. Metadata is
   * layered in priority order: format-specific pass-through (lowest), explicit
   * extraction fields, user-supplied additional metadata (highest).
   */
  private Map<String, Object> buildBaseMetadata(ExtractedDocument document,
                                                String source) {
    Map<String, Object> metadata = new LinkedHashMap<>();
    Metadata extractionMetadata = document.metadata();

    // 1. Format-specific pass-through (lowest priority among structured fields)
    // ~keep
    if (extractionMetadata != null) {
      addFormatSpecificMetadata(metadata, extractionMetadata);
    }

    // 2. Explicit fields from ExtractedDocument and Metadata ~keep
    metadata.put("source", source);
    metadata.put("mime_type", document.mimeType());
    metadata.put("page_count",
                 document.counts() != null ? document.counts().pages() : 0L);

    List<String> detectedLanguages = document.detectedLanguages();
    metadata.put(
        "detected_languages",
        detectedLanguages != null ? String.join(", ", detectedLanguages) : "");

    if (document.qualityScore() != null) {
      metadata.put("quality_score", document.qualityScore());
    }

    if (extractionMetadata != null) {
      addExtractionMetadata(metadata, extractionMetadata);
    }

    List<?> tables = document.tables();
    metadata.put("table_count", tables != null ? tables.size() : 0);
    if (tables != null && !tables.isEmpty()) {
      metadata.put("tables", toJson(tables));
    }

    if (document.extractedKeywords() != null) {
      metadata.put("extracted_keywords", toJson(document.extractedKeywords()));
    }
    if (document.processingWarnings() != null) {
      metadata.put("processing_warnings",
                   toJson(document.processingWarnings()));
    }

    // 3. User-supplied additional metadata (highest priority) ~keep
    metadata.putAll(additionalMetadata);

    return metadata;
  }

  /**
   * Copies the typed {@link Metadata} fields into the output map, skipping any
   * that are {@code null}. List-valued fields are joined into comma-separated
   * strings.
   */
  private void addExtractionMetadata(Map<String, Object> metadata,
                                     Metadata extractionMetadata) {
    if (extractionMetadata.title() != null) {
      metadata.put("title", extractionMetadata.title());
    }
    if (extractionMetadata.subject() != null) {
      metadata.put("subject", extractionMetadata.subject());
    }
    if (extractionMetadata.authors() != null) {
      metadata.put("authors", String.join(", ", extractionMetadata.authors()));
    }
    if (extractionMetadata.keywords() != null) {
      metadata.put("keywords",
                   String.join(", ", extractionMetadata.keywords()));
    }
    if (extractionMetadata.language() != null) {
      metadata.put("language", extractionMetadata.language());
    }
    if (extractionMetadata.createdAt() != null) {
      metadata.put("created_at", extractionMetadata.createdAt());
    }
    if (extractionMetadata.modifiedAt() != null) {
      metadata.put("modified_at", extractionMetadata.modifiedAt());
    }
    if (extractionMetadata.createdBy() != null) {
      metadata.put("created_by", extractionMetadata.createdBy());
    }
    if (extractionMetadata.modifiedBy() != null) {
      metadata.put("modified_by", extractionMetadata.modifiedBy());
    }
    if (extractionMetadata.category() != null) {
      metadata.put("category", extractionMetadata.category());
    }
    if (extractionMetadata.tags() != null) {
      metadata.put("tags", String.join(", ", extractionMetadata.tags()));
    }
    if (extractionMetadata.documentVersion() != null) {
      metadata.put("document_version", extractionMetadata.documentVersion());
    }
    if (extractionMetadata.abstractText() != null) {
      metadata.put("abstract_text", extractionMetadata.abstractText());
    }
    if (extractionMetadata.outputFormat() != null) {
      metadata.put("output_format", extractionMetadata.outputFormat());
    }
  }

  /**
   * Passes through format-specific metadata from the extraction result.
   * Primitives are added directly; complex types (lists, maps) are serialized
   * to JSON strings.
   */
  private void addFormatSpecificMetadata(Map<String, Object> metadata,
                                         Metadata extractionMetadata) {
    Map<String, Object> additional = extractionMetadata.additional();
    if (additional == null) {
      return;
    }
    for (Map.Entry<String, Object> entry : additional.entrySet()) {
      Object value = entry.getValue();
      if (value instanceof String || value instanceof Integer ||
          value instanceof Long || value instanceof Float ||
          value instanceof Double || value instanceof Boolean) {
        metadata.put(entry.getKey(), value);
      } else if (value instanceof List || value instanceof Map) {
        metadata.put(entry.getKey(), toJson(value));
      }
    }
  }

  private static String toJson(Object value) {
    try {
      return OBJECT_MAPPER.writeValueAsString(value);
    } catch (JsonProcessingException e) {
      throw new RuntimeException("Failed to serialize to JSON", e);
    }
  }
}
