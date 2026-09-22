use super::super::*;

#[test]
fn test_filter_leaked_metadata_clean_text() {
    let text = "This is normal text without any metadata patterns.";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert_eq!(result, text);
}

#[test]
fn test_filter_leaked_metadata_removes_whitepoint() {
    let text = "Hello World\nWhitePoint [ 0.95 1.0 1.09 ]\nMore text";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.contains("Hello World"));
    assert!(result.contains("More text"));
    assert!(!result.contains("WhitePoint"));
}

#[test]
fn test_filter_leaked_metadata_removes_calrgb() {
    let text = "Text\nCalRGB /WhitePoint [ 1 1 1 ]\nMore";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.contains("Text"));
    assert!(result.contains("More"));
    assert!(!result.contains("CalRGB"));
}

#[test]
fn test_filter_leaked_metadata_preserves_normal_lines() {
    let text = "The Matrix is a movie\nGamma rays from space";
    // These lines contain metadata keywords but not in metadata format ~keep
    let result = PdfDocument::filter_leaked_metadata(text);
    // "The Matrix is a movie" should be preserved (doesn't start with "Matrix") ~keep
    assert!(result.contains("The Matrix is a movie"));
}

#[test]
fn test_filter_leaked_metadata_blackpoint() {
    let text = "BlackPoint [ 0 0 0 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_gamma() {
    let text = "Some text\nGamma [ 2.2 2.2 2.2 ]\nMore text";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(!result.contains("Gamma"));
    assert!(result.contains("Some text"));
    assert!(result.contains("More text"));
}

#[test]
fn test_filter_leaked_metadata_matrix_start_line() {
    let text = "Matrix [ 1 0 0 1 0 0 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_calgray() {
    let text = "CalGray /WhitePoint [ 1 1 1 ]";
    let result = PdfDocument::filter_leaked_metadata(text);
    assert!(!result.contains("CalGray"));
}

#[test]
fn test_filter_leaked_metadata_whitepoint_with_slash() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint /something");
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_whitepoint_with_angle() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint << /Key /Value >>");
    assert!(result.trim().is_empty());
}

#[test]
fn test_filter_leaked_metadata_empty_metadata_value() {
    let result = PdfDocument::filter_leaked_metadata("WhitePoint");
    assert!(result.trim().is_empty());
}
