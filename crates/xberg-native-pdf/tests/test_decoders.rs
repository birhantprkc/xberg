//! Integration tests for stream decoders.
//!
//! Tests all decoders with various scenarios including:
//! - Individual decoder functionality
//! - Filter pipelines (multiple decoders chained)
//! - Edge cases and error handling
//! - Integration with Object::decode_stream_data()

use bytes::Bytes;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::collections::HashMap;
use std::io::Write;
use weezl::{BitOrder, encode::Encoder as LzwEncoder};
use xberg_native_pdf::decoders::{
    Ascii85Decoder, AsciiHexDecoder, DctDecoder, FlateDecoder, LzwDecoder, RunLengthDecoder, StreamDecoder,
    decode_stream,
};
use xberg_native_pdf::object::Object;

#[test]
fn test_flate_decoder_integration() {
    let decoder = FlateDecoder::default();

    let original = b"This is a test of FlateDecode compression in a PDF stream.";

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    let decoded = decoder.decode(&compressed, 0).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_ascii_hex_decoder_integration() {
    let decoder = AsciiHexDecoder;

    let test_cases = vec![
        (b"48656C6C6F20576F726C64".as_slice(), b"Hello World".as_slice()),
        (b"54657374".as_slice(), b"Test".as_slice()),
        (b"414243444546".as_slice(), b"ABCDEF".as_slice()),
    ];

    for (input, expected) in test_cases {
        let decoded = decoder.decode(input, 0).unwrap();
        assert_eq!(decoded, expected);
    }
}

#[test]
fn test_ascii85_decoder_integration() {
    let decoder = Ascii85Decoder;

    let decoded = decoder.decode(b"z", 0).unwrap();
    assert_eq!(decoded, b"\x00\x00\x00\x00");

    let decoded = decoder.decode(b"<+U,m", 0).unwrap();
    assert_eq!(decoded, b"Test");
}

#[test]
fn test_lzw_decoder_integration() {
    let decoder = LzwDecoder;

    let original = b"ABABABABABABABAB";
    let mut encoder = LzwEncoder::new(BitOrder::Msb, 8);
    let compressed = encoder.encode(original).unwrap();

    let decoded = decoder.decode(&compressed, 0).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_runlength_decoder_integration() {
    let decoder = RunLengthDecoder;

    let input = vec![2, b'A', b'B', b'C'];
    let decoded = decoder.decode(&input, 0).unwrap();
    assert_eq!(decoded, b"ABC");

    let input = vec![250, b'X'];
    let decoded = decoder.decode(&input, 0).unwrap();
    assert_eq!(decoded, b"XXXXXXX");
}

#[test]
fn test_dct_decoder_integration() {
    let decoder = DctDecoder;

    let jpeg_data = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01";
    let decoded = decoder.decode(jpeg_data, 0).unwrap();
    assert_eq!(decoded, jpeg_data);
}

#[test]
fn test_filter_pipeline_single() {
    let data = b"48656C6C6F";
    let filters = vec!["ASCIIHexDecode".to_string()];

    let decoded = decode_stream(data, &filters).unwrap();
    assert_eq!(decoded, b"Hello");
}

#[test]
fn test_filter_pipeline_multiple() {
    let original = b"Hello, World!";
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    let hex_encoded: String = compressed.iter().map(|b| format!("{:02X}", b)).collect();

    let filters = vec!["ASCIIHexDecode".to_string(), "FlateDecode".to_string()];
    let decoded = decode_stream(hex_encoded.as_bytes(), &filters).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_filter_pipeline_unsupported() {
    let data = b"test";
    let filters = vec!["NonExistentFilter".to_string()];

    let result = decode_stream(data, &filters);
    assert!(result.is_err());
}

#[test]
fn test_object_stream_decode_no_filter() {
    let mut dict = HashMap::new();
    dict.insert("Length".to_string(), Object::Integer(13));

    let stream = Object::Stream {
        dict,
        data: Bytes::from_static(b"Hello, World!"),
    };

    let decoded = stream.decode_stream_data().unwrap();
    assert_eq!(decoded, b"Hello, World!");
}

#[test]
fn test_object_stream_decode_with_flate() {
    let original = b"This is compressed data in a PDF stream.";
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut dict = HashMap::new();
    dict.insert("Length".to_string(), Object::Integer(compressed.len() as i64));
    dict.insert("Filter".to_string(), Object::Name("FlateDecode".to_string()));

    let stream = Object::Stream {
        dict,
        data: Bytes::from(compressed),
    };

    let decoded = stream.decode_stream_data().unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_object_stream_decode_with_multiple_filters() {
    let original = b"Test data";

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    let hex_encoded: String = compressed.iter().map(|b| format!("{:02X}", b)).collect();

    let mut dict = HashMap::new();
    dict.insert(
        "Filter".to_string(),
        Object::Array(vec![
            Object::Name("ASCIIHexDecode".to_string()),
            Object::Name("FlateDecode".to_string()),
        ]),
    );

    let stream = Object::Stream {
        dict,
        data: Bytes::from(hex_encoded.into_bytes()),
    };

    let decoded = stream.decode_stream_data().unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_object_stream_decode_error_not_stream() {
    let obj = Object::Integer(42);
    let result = obj.decode_stream_data();
    assert!(result.is_err());
}

#[test]
fn test_all_decoders_name() {
    assert_eq!(FlateDecoder::default().name(), "FlateDecode");
    assert_eq!(AsciiHexDecoder.name(), "ASCIIHexDecode");
    assert_eq!(Ascii85Decoder.name(), "ASCII85Decode");
    assert_eq!(LzwDecoder.name(), "LZWDecode");
    assert_eq!(RunLengthDecoder.name(), "RunLengthDecode");
    assert_eq!(DctDecoder.name(), "DCTDecode");
}

#[test]
fn test_decode_stream_empty_filters() {
    let data = b"No compression here!";
    let decoded = decode_stream(data, &[]).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_complex_filter_pipeline() {
    let original = b"Complex!";

    let mut lzw_encoder = LzwEncoder::new(BitOrder::Msb, 8);
    let lzw_compressed = lzw_encoder.encode(original).unwrap();

    let mut flate_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    flate_encoder.write_all(&lzw_compressed).unwrap();
    let double_compressed = flate_encoder.finish().unwrap();

    let hex_encoded: String = double_compressed.iter().map(|b| format!("{:02X}", b)).collect();

    let filters = vec![
        "ASCIIHexDecode".to_string(),
        "FlateDecode".to_string(),
        "LZWDecode".to_string(),
    ];

    let decoded = decode_stream(hex_encoded.as_bytes(), &filters).unwrap();
    assert_eq!(decoded, original);
}
