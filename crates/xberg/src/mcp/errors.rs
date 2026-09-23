//! MCP error mapping.
//!
//! This module provides functions to map Xberg errors to MCP error responses.

use crate::XbergError;
use crate::error::McpErrorCategory;
use rmcp::ErrorData as McpError;
use std::fmt::Write;

/// Map Xberg errors to MCP error responses with appropriate error codes.
///
/// This function ensures different error types are properly differentiated in MCP responses:
/// - `Validation` errors → `INVALID_PARAMS` (-32602)
/// - `UnsupportedFormat` errors → `INVALID_PARAMS` (-32602)
/// - `Parsing` errors → `PARSE_ERROR` (-32700)
/// - `Io` errors → `INTERNAL_ERROR` (-32603) with context preserved
/// - `Cancelled` errors → `REQUEST_CANCELLED` (-32800)
/// - All other errors → `INTERNAL_ERROR` (-32603)
///
/// The error message and source chain are preserved to aid debugging.
#[doc(hidden)]
pub(crate) fn map_xberg_error_to_mcp(error: XbergError) -> McpError {
    let category = error.mcp_error_category();
    let message = mcp_error_message(error);

    match category {
        McpErrorCategory::InvalidParams => McpError::invalid_params(message, None),
        McpErrorCategory::ParseError => McpError::parse_error(message, None),
        McpErrorCategory::Cancelled => McpError {
            code: rmcp::model::ErrorCode(-32800),
            message: message.into(),
            data: None,
        },
        McpErrorCategory::Internal => McpError::internal_error(message, None),
    }
}

/// Formats `"{prefix}: {message}"`, appending `" (caused by: <source>)"` when the error carries
/// a source. Shared by every [`XbergError`] variant that has a message plus an optional source, so
/// all of them render identically. ~keep
fn with_source_suffix(prefix: &str, message: &str, source: Option<Box<dyn std::error::Error + Send + Sync>>) -> String {
    let mut error_message = format!("{}: {}", prefix, message);
    if let Some(src) = source {
        let _ = write!(error_message, " (caused by: {})", src);
    }
    error_message
}

/// Builds the human-readable message for an [`XbergError`], as reported by the MCP error
/// conversion. The JSON-RPC error code for the same error is selected separately, via
/// [`XbergError::mcp_error_category`]; see [`map_xberg_error_to_mcp`].
fn mcp_error_message(error: XbergError) -> String {
    match error {
        XbergError::Validation { message, source } => with_source_suffix("Validation error", &message, source),

        XbergError::UnsupportedFormat(mime_type) => format!("Unsupported format: {}", mime_type),

        XbergError::MissingDependency(dep) => format!(
            "Missing required dependency: {}. Please install it to use this feature.",
            dep
        ),

        XbergError::Parsing { message, source } => with_source_suffix("Parsing error", &message, source),

        // OSError/RuntimeError must bubble up - system errors need user reports ~keep
        XbergError::Io(io_err) => format!("System I/O error: {}", io_err),

        XbergError::Ocr { message, source } => with_source_suffix("OCR processing error", &message, source),

        XbergError::Cache { message, source } => with_source_suffix("Cache error", &message, source),

        XbergError::ImageProcessing { message, source } => {
            with_source_suffix("Image processing error", &message, source)
        }

        XbergError::Serialization { message, source } => with_source_suffix("Serialization error", &message, source),

        XbergError::Embedding { message, source } => with_source_suffix("Embedding error", &message, source),

        XbergError::Plugin { message, plugin_name } => {
            format!("Plugin '{}' error: {}", plugin_name, message)
        }

        XbergError::LockPoisoned(msg) => format!("Internal lock poisoned: {}", msg),

        XbergError::Timeout { elapsed_ms, limit_ms } => {
            format!("Extraction timed out after {elapsed_ms}ms (limit: {limit_ms}ms)")
        }

        XbergError::Other(msg) => msg,

        XbergError::Cancelled => "Extraction cancelled".to_string(),

        XbergError::Security { message, source } => with_source_suffix("Security violation", &message, source),

        XbergError::Transcription { message, source } => with_source_suffix("Transcription error", &message, source),

        XbergError::Reranking { message, source } => with_source_suffix("Reranking error", &message, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_validation_error_to_invalid_params() {
        let error = XbergError::validation("invalid file path");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32602);
        assert!(mcp_error.message.contains("Validation error"));
        assert!(mcp_error.message.contains("invalid file path"));
    }

    #[test]
    fn test_map_validation_error_with_source_preserves_chain() {
        let source = std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad param");
        let error = XbergError::validation_with_source("invalid configuration", source);
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32602);
        assert!(mcp_error.message.contains("Validation error"));
        assert!(mcp_error.message.contains("invalid configuration"));
        assert!(mcp_error.message.contains("caused by"));
    }

    #[test]
    fn test_map_unsupported_format_to_invalid_params() {
        let error = XbergError::UnsupportedFormat("application/unknown".to_string());
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32602);
        assert!(mcp_error.message.contains("Unsupported format"));
        assert!(mcp_error.message.contains("application/unknown"));
    }

    #[test]
    fn test_map_missing_dependency_to_invalid_params() {
        let error = XbergError::MissingDependency("tesseract".to_string());
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32602);
        assert!(mcp_error.message.contains("Missing required dependency"));
        assert!(mcp_error.message.contains("tesseract"));
        assert!(mcp_error.message.contains("Please install"));
    }

    #[test]
    fn test_map_parsing_error_to_parse_error() {
        let error = XbergError::parsing("corrupt PDF file");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32700);
        assert!(mcp_error.message.contains("Parsing error"));
        assert!(mcp_error.message.contains("corrupt PDF file"));
    }

    #[test]
    fn test_map_parsing_error_with_source_preserves_chain() {
        let source = std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed data");
        let error = XbergError::parsing_with_source("failed to parse document", source);
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32700);
        assert!(mcp_error.message.contains("Parsing error"));
        assert!(mcp_error.message.contains("failed to parse document"));
        assert!(mcp_error.message.contains("caused by"));
    }

    #[test]
    fn test_map_io_error_to_internal_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error = XbergError::Io(io_error);
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("System I/O error"));
        assert!(mcp_error.message.contains("file not found"));
    }

    #[test]
    fn test_map_ocr_error_to_internal_error() {
        let error = XbergError::ocr("tesseract failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("OCR processing error"));
        assert!(mcp_error.message.contains("tesseract failed"));
    }

    #[test]
    fn test_map_cache_error_to_internal_error() {
        let error = XbergError::cache("cache write failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Cache error"));
        assert!(mcp_error.message.contains("cache write failed"));
    }

    #[test]
    fn test_map_image_processing_error_to_internal_error() {
        let error = XbergError::image_processing("resize failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Image processing error"));
        assert!(mcp_error.message.contains("resize failed"));
    }

    #[test]
    fn test_map_serialization_error_to_internal_error() {
        let error = XbergError::serialization("JSON encode failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Serialization error"));
        assert!(mcp_error.message.contains("JSON encode failed"));
    }

    #[test]
    fn test_map_embedding_error_to_internal_error() {
        let error = XbergError::embedding("Model failed to load");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Embedding error"));
        assert!(mcp_error.message.contains("Model failed to load"));
    }

    #[test]
    fn test_map_plugin_error_to_internal_error() {
        let error = XbergError::Plugin {
            message: "extraction failed".to_string(),
            plugin_name: "pdf-extractor".to_string(),
        };
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Plugin 'pdf-extractor' error"));
        assert!(mcp_error.message.contains("extraction failed"));
    }

    #[test]
    fn test_map_lock_poisoned_error_to_internal_error() {
        let error = XbergError::LockPoisoned("registry lock poisoned".to_string());
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Internal lock poisoned"));
        assert!(mcp_error.message.contains("registry lock poisoned"));
    }

    #[test]
    fn test_map_other_error_to_internal_error() {
        let error = XbergError::Other("unexpected error".to_string());
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("unexpected error"));
    }

    #[test]
    fn test_map_cancelled_to_request_cancelled() {
        let error = XbergError::Cancelled;
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32800);
        assert!(mcp_error.message.contains("cancelled"));
    }

    #[test]
    fn should_map_security_error_to_invalid_params() {
        let error = XbergError::security("zip bomb detected");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32602);
        assert!(mcp_error.message.contains("Security violation"));
        assert!(mcp_error.message.contains("zip bomb detected"));
    }

    #[test]
    fn should_map_timeout_error_to_internal_error_with_exact_message() {
        let error = XbergError::Timeout {
            elapsed_ms: 5000,
            limit_ms: 3000,
        };
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert_eq!(mcp_error.message, "Extraction timed out after 5000ms (limit: 3000ms)");
    }

    #[test]
    fn should_map_transcription_error_to_internal_error() {
        let error = XbergError::transcription("whisper backend failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Transcription error"));
        assert!(mcp_error.message.contains("whisper backend failed"));
    }

    #[test]
    fn should_map_reranking_error_to_internal_error() {
        let error = XbergError::reranking("cross-encoder failed");
        let mcp_error = map_xberg_error_to_mcp(error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("Reranking error"));
        assert!(mcp_error.message.contains("cross-encoder failed"));
    }

    #[test]
    fn test_error_type_differentiation() {
        let validation = XbergError::validation("test");
        let parsing = XbergError::parsing("test");
        let io = XbergError::Io(std::io::Error::other("test"));

        let val_mcp = map_xberg_error_to_mcp(validation);
        let parse_mcp = map_xberg_error_to_mcp(parsing);
        let io_mcp = map_xberg_error_to_mcp(io);

        assert_eq!(val_mcp.code.0, -32602);
        assert_eq!(parse_mcp.code.0, -32700);
        assert_eq!(io_mcp.code.0, -32603);

        assert_ne!(val_mcp.code.0, parse_mcp.code.0);
        assert_ne!(val_mcp.code.0, io_mcp.code.0);
        assert_ne!(parse_mcp.code.0, io_mcp.code.0);
    }

    #[test]
    fn test_error_mapping_preserves_error_context() {
        let validation_error = XbergError::validation("invalid file path");
        let mcp_error = map_xberg_error_to_mcp(validation_error);

        assert!(mcp_error.message.contains("invalid file path"));
    }

    #[test]
    fn test_io_errors_bubble_up_as_internal() {
        // OSError/RuntimeError must bubble up - system errors need user reports ~keep
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let xberg_error = XbergError::Io(io_error);
        let mcp_error = map_xberg_error_to_mcp(xberg_error);

        assert_eq!(mcp_error.code.0, -32603);
        assert!(mcp_error.message.contains("System I/O error"));
    }

    #[test]
    fn test_all_error_variants_have_mappings() {
        let errors = vec![
            XbergError::validation("test"),
            XbergError::UnsupportedFormat("test/unknown".to_string()),
            XbergError::MissingDependency("test-dep".to_string()),
            XbergError::parsing("test"),
            XbergError::Io(std::io::Error::other("test")),
            XbergError::ocr("test"),
            XbergError::cache("test"),
            XbergError::image_processing("test"),
            XbergError::serialization("test"),
            XbergError::Plugin {
                message: "test".to_string(),
                plugin_name: "test-plugin".to_string(),
            },
            XbergError::LockPoisoned("test".to_string()),
            XbergError::Other("test".to_string()),
            XbergError::Cancelled,
            XbergError::reranking("test rerank error"),
        ];

        for error in errors {
            let mcp_error = map_xberg_error_to_mcp(error);

            assert!(mcp_error.code.0 < 0, "Error code should be negative");

            assert!(!mcp_error.message.is_empty());
        }
    }
}
