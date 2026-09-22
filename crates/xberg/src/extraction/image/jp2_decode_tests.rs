//! JP2/J2K decode-path tests for the image extraction module, split out of `image.rs`
//! to keep that file under the line-count limit.

use super::*;

#[test]
fn jp2_peak_counts_encoded_input_between_old_and_new_thresholds() {
    let peak = jp2_peak_live_bytes(10, 10, 1, false, 100).expect("valid JP2 peak");
    let limits = SecurityLimits {
        max_content_size: 450,
        ..Default::default()
    };

    let error = ImageDecodeBudget::from_security_limits(&limits)
        .validate(10, 10, peak)
        .expect_err("encoded JP2 bytes must remain live alongside gray-to-RGB conversion");

    assert!(matches!(error, XbergError::Validation { .. }));
}

#[test]
fn jbig2_peaks_count_encoded_input_and_gray_to_rgb_conversion() {
    let decode_peak = jbig2_gray_peak_live_bytes(10, 10, 100).expect("valid JBIG2 decode peak");
    let conversion_peak = jbig2_rgb_peak_live_bytes(10, 10, 100).expect("valid JBIG2 RGB peak");

    assert_eq!(decode_peak, 200);
    assert_eq!(conversion_peak, 500);
    let limits = SecurityLimits {
        max_content_size: 450,
        ..Default::default()
    };
    let error = ImageDecodeBudget::from_security_limits(&limits)
        .validate(10, 10, conversion_peak)
        .expect_err("encoded JBIG2, gray pixels, and RGB pixels must be live together");
    assert!(matches!(error, XbergError::Validation { .. }));
}

#[test]
fn test_decode_jp2_to_rgb() {
    let Some(bytes) = crate::utils::read_test_fixture("images/rust-logo-512x512-blk.jp2") else {
        return;
    };
    let rgb = decode_jp2_to_rgb(&bytes).expect("Should decode JP2 to RGB");
    assert_eq!(rgb.width(), 512);
    assert_eq!(rgb.height(), 512);
}

#[test]
fn test_is_j2k() {
    assert!(!is_j2k(&[]));
    assert!(!is_j2k(&[0xFF]));
    assert!(is_j2k(&[0xFF, 0x4F, 0xFF, 0x51, 0x00]));
    assert!(!is_j2k(&[0xFF, 0x4F, 0x00, 0x51]));
}

#[test]
fn test_jbig2_magic_detection() {
    assert!(is_jbig2(&[0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01]));
    assert!(!is_jbig2(&[0x89, 0x50, 0x4E, 0x47]));
    assert!(!is_jbig2(&[]));
    assert!(!is_jbig2(&[0x97, 0x4A]));
}
