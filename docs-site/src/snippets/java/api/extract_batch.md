```java title="Java"
import io.xberg.ExtractInput;
import io.xberg.ExtractInputKind;
import io.xberg.ExtractionConfig;
import io.xberg.Xberg;
import java.util.List;

var inputs = List.of(
    ExtractInput.builder()
        .withKind(ExtractInputKind.URI)
        .withUri("report.pdf")
        .build(),
    ExtractInput.builder()
        .withKind(ExtractInputKind.URI)
        .withUri("notes.txt")
        .build()
);

var output = Xberg.extractBatch(inputs, ExtractionConfig.builder().build());
for (var result : output.results()) {
    System.out.println(result.content());
}
```
