// Prove an installed xberg-ffi native library actually extracts documents,
// not just that the JVM can dlopen() it and resolve every required symbol.
//
// io.xberg.NativeLib already validates, in a static initializer that runs the
// first time any io.xberg class is touched, that every REQUIRED_SYMBOLS name
// resolves via SymbolLookup -- so simply loading the class already proves the
// shared library opens and every expected C symbol is present by name. But
// symbol presence does not prove a symbol's calling convention or the
// extraction pipeline behind it actually works: a signature mismatch, a panic
// inside Rust, or a subtly broken vendored shared-lib closure (ONNX Runtime,
// libheif, ...) would still pass that check. This program therefore runs a
// real extraction against a known fixture and asserts the exact expected text
// comes back.
//
// Usage: java --enable-preview --enable-native-access=ALL-UNNAMED Smoke <fixture-path> <expected-substring>
// Exit code is non-zero, with a message on stderr, on any failure: class
// init failure (bad/missing native lib), extract() throwing, an empty
// result, or a content mismatch.
import io.xberg.ExtractInput;
import io.xberg.ExtractInputKind;
import io.xberg.ExtractionConfig;
import io.xberg.ExtractionResult;
import io.xberg.JsonUtil;
import io.xberg.Xberg;

public final class Smoke {
    private Smoke() {
    }

    private static void fail(final String message) {
        System.err.println("SMOKE TEST FAILED: " + message);
        System.exit(1);
    }

    public static void main(final String[] args) throws Exception {
        if (args.length != 2) {
            fail("usage: Smoke <fixture-path> <expected-substring>");
            return;
        }
        final String fixturePath = args[0];
        final String expectedSubstring = args[1];

        final ExtractInput input = ExtractInput.builder()
            .withKind(ExtractInputKind.URI)
            .withUri(fixturePath)
            .build();
        final ExtractionConfig config = JsonUtil.fromJson("{}", ExtractionConfig.class);

        final ExtractionResult result;
        try {
            result = Xberg.extract(input, config);
        } catch (Exception e) {
            fail("extract() threw for fixture " + fixturePath + ": " + e);
            return;
        }

        if (result.results() == null || result.results().isEmpty()) {
            fail("extract() returned zero results for fixture " + fixturePath);
            return;
        }

        final String content = result.results().get(0).content();
        if (content == null || !content.contains(expectedSubstring)) {
            fail("expected substring '" + expectedSubstring + "' not found in extracted content for fixture "
                + fixturePath + "; got: " + content);
            return;
        }

        System.out.println("SMOKE TEST PASSED: found '" + expectedSubstring + "' in extracted content");
    }
}
