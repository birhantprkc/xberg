```java title="Java"
import io.xberg.Xberg;
import io.xberg.ExtractInputKind;
import io.xberg.ExtractionResult;
import io.xberg.ExtractedDocument;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractInput;

// Note: the Java binding has no config-file discovery helper. Build the
// config object directly (or load `xberg.toml`/`xberg.yaml`/`xberg.json`
// yourself and parse it) and pass it to `extract`.
ExtractionConfig config = ExtractionConfig.builder().build();
ExtractionResult output = Xberg.extract(
    ExtractInput.builder().withKind(ExtractInputKind.URI).withUri("document.pdf").build(),
    config
);
ExtractedDocument result = output.results().get(0);
```
