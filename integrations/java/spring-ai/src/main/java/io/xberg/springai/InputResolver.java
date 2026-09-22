package io.xberg.springai;

import io.xberg.ExtractInput;
import io.xberg.ExtractInputKind;
import java.io.IOException;
import java.net.URLConnection;
import java.util.ArrayList;
import java.util.List;
import org.springframework.core.io.FileSystemResource;
import org.springframework.core.io.Resource;

/**
 * Builds Xberg {@link ExtractInput} values from Spring {@link Resource}s and
 * resolves each resource's human-readable source and MIME type.
 *
 * <p>
 * Split out of {@link XbergDocumentReader} to keep that class within the repo's
 * type-length budget; holds only the state {@link XbergDocumentReader.Builder}
 * already validated (resources, and an optional single-resource MIME override).
 * ~keep
 */
final class InputResolver {

  private final List<Resource> resources;
  private final String mimeType;

  InputResolver(List<Resource> resources, String mimeType) {
    this.resources = resources;
    this.mimeType = mimeType;
  }

  /**
   * Builds an {@link ExtractInput} per resource. A {@link FileSystemResource}
   * takes the URI fast path (Xberg reads the file directly); every other
   * resource is read into memory and submitted as bytes.
   */
  List<ExtractInput> buildInputs() throws IOException {
    List<ExtractInput> inputs = new ArrayList<>(resources.size());
    for (Resource resource : resources) {
      inputs.add(toInput(resource));
    }
    return inputs;
  }

  private ExtractInput toInput(Resource resource) throws IOException {
    if (resource instanceof FileSystemResource) {
      return ExtractInput.builder()
          .withKind(ExtractInputKind.URI)
          .withUri(resource.getFile().getAbsolutePath())
          .withFilename(resource.getFilename())
          .build();
    }
    byte[] bytes = resource.getInputStream().readAllBytes();
    return ExtractInput.builder()
        .withKind(ExtractInputKind.BYTES)
        .withBytes(bytes)
        .withMimeType(resolveMimeType(resource))
        .withFilename(resource.getFilename())
        .build();
  }

  String resolveSource(Resource resource) {
    String filename = resource.getFilename();
    if (filename != null) {
      return filename;
    }
    return "bytes://" + resolveMimeType(resource);
  }

  String resolveMimeType(Resource resource) {
    if (mimeType != null && resources.size() == 1) {
      return mimeType;
    }
    String filename = resource.getFilename();
    if (filename != null) {
      String guessed = URLConnection.guessContentTypeFromName(filename);
      if (guessed != null) {
        return guessed;
      }
      return "application/octet-stream";
    }
    throw new IllegalStateException("Cannot resolve MIME type: no explicit "
                                    + "mimeType and resource has no filename");
  }
}
