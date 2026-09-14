```java title="Java"
import io.xberg.ExtractInput;
import io.xberg.ExtractInputKind;
import io.xberg.ExtractedDocument;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractionResult;
import io.xberg.Keyword;
import io.xberg.KeywordAlgorithm;
import io.xberg.KeywordConfig;
import io.xberg.Xberg;
import io.xberg.XbergRsException;

public class KeywordExtractionExample {
    public static void main(String[] args) {
        ExtractionConfig config = ExtractionConfig.builder()
            .withKeywords(KeywordConfig.builder()
                .withAlgorithm(KeywordAlgorithm.YAKE)
                .withMaxKeywords(10L)
                .withMinScore(0.3f)
                .build())
            .build();

        ExtractInput input = ExtractInput.builder()
            .withKind(ExtractInputKind.URI)
            .withUri("research_paper.pdf")
            .build();

        try {
            ExtractionResult output = Xberg.extract(input, config);
            ExtractedDocument result = output.results().get(0);

            if (result.extractedKeywords() == null) {
                System.out.println("No keywords met the configured minimum score.");
                return;
            }
            for (Keyword keyword : result.extractedKeywords()) {
                System.out.printf("%s: %.3f (%s)%n",
                    keyword.text(), keyword.score(), keyword.algorithm());
            }
        } catch (XbergRsException e) {
            System.err.println("Keyword extraction failed: " + e.getMessage());
        }
    }
}
```
