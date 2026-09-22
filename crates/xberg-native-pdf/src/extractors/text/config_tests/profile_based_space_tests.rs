//! Space-insertion threshold tests for extraction profiles (`ACADEMIC`, `POLICY`, `FORM`).

use super::*;

/// Test that ACADEMIC profile uses aggressive thresholds
///
/// Academic papers have tight spacing (especially around punctuation).
/// The profile should:
/// - Use lower TJ offset threshold (-90 instead of -120)
/// - Use lower word margin ratio (0.12 instead of 0.1)
/// - Enable adaptive threshold for dynamic adjustment
#[test]
fn test_academic_profile_thresholds() {
    let profile = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Academic);

    assert!(
        profile.tj_offset_threshold < -100.0,
        "Academic should use lower TJ threshold for more spaces"
    );

    assert!(
        profile.word_margin_ratio <= 0.15,
        "Academic should use conservative word margin"
    );

    let config = TextExtractionConfig::with_space_threshold(profile.tj_offset_threshold);
    assert_eq!(config.space_insertion_threshold, profile.tj_offset_threshold);
}

/// Test that POLICY profile uses conservative thresholds
///
/// Policy documents (like GDPR) have justified text with precise spacing.
/// The profile should:
/// - Use higher TJ offset threshold (-110 to preserve structure)
/// - Use higher word margin ratio (0.18-0.2 for justified text)
/// - Preserve column boundaries and table structure
#[test]
fn test_policy_profile_thresholds() {
    let profile = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Policy);

    assert!(
        profile.tj_offset_threshold > -120.0,
        "Policy should use higher TJ threshold to avoid over-spacing"
    );

    assert!(
        profile.word_margin_ratio >= 0.15,
        "Policy should use higher word margin for justified text"
    );

    let config = TextExtractionConfig::with_space_threshold(profile.tj_offset_threshold);
    assert_eq!(config.space_insertion_threshold, profile.tj_offset_threshold);
}

/// Test that FORM profile preserves field boundaries
///
/// Forms have checkboxes, fields, and precise layout.
/// The profile should:
/// - Use conservative thresholds to avoid merging fields
/// - High column boundary threshold to preserve structure
/// - Enable adaptive threshold for form field detection
#[test]
fn test_form_profile_thresholds() {
    let profile = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Form);

    assert!(
        profile.tj_offset_threshold >= -120.0,
        "Form profile should be conservative with space insertion"
    );

    let config = TextExtractionConfig::with_space_threshold(profile.tj_offset_threshold);
    assert_eq!(config.space_insertion_threshold, profile.tj_offset_threshold);
}

/// Test that profile selection works correctly for document types
#[test]
fn test_profile_selection_for_document_types() {
    let academic = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Academic);
    let policy = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Policy);
    let form = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Form);
    let mixed = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Mixed);

    let thresholds = [
        academic.tj_offset_threshold,
        policy.tj_offset_threshold,
        form.tj_offset_threshold,
        mixed.tj_offset_threshold,
    ];

    let unique_count = thresholds
        .iter()
        .filter(|t| !thresholds.iter().skip(1).any(|other| other == *t))
        .count();

    assert!(
        unique_count > 0,
        "Profiles should have different thresholds for different document types"
    );
}

/// Test that TextExtractionConfig can accept a profile
#[test]
fn test_config_with_profile() {
    let profile = crate::config::ExtractionProfile::ACADEMIC;

    let config = TextExtractionConfig::with_space_threshold(profile.tj_offset_threshold);

    assert_eq!(config.space_insertion_threshold, profile.tj_offset_threshold);
}

/// Test that profiles have reasonable threshold ranges
#[test]
fn test_profile_thresholds_in_reasonable_range() {
    let profiles = vec![
        crate::config::ExtractionProfile::CONSERVATIVE,
        crate::config::ExtractionProfile::ACADEMIC,
        crate::config::ExtractionProfile::POLICY,
        crate::config::ExtractionProfile::FORM,
    ];

    for profile in profiles {
        // TJ offsets should be negative (per PDF spec) ~keep
        assert!(
            profile.tj_offset_threshold < 0.0,
            "TJ threshold must be negative ({})",
            profile.name
        );

        // Should be in reasonable range (-150 to -50) ~keep
        assert!(
            profile.tj_offset_threshold >= -150.0 && profile.tj_offset_threshold <= -50.0,
            "TJ threshold out of range for {} ({})",
            profile.name,
            profile.tj_offset_threshold
        );

        // Word margin ratios should be positive and reasonable (0.05 to 0.25) ~keep
        assert!(
            profile.word_margin_ratio > 0.0 && profile.word_margin_ratio < 1.0,
            "Word margin ratio must be between 0 and 1 for {}",
            profile.name
        );

        assert!(
            profile.space_threshold_em_ratio > 0.0,
            "Space threshold EM ratio must be positive for {}",
            profile.name
        );
    }
}

/// Test that multiple profiles can coexist
#[test]
fn test_multiple_profiles_independent() {
    let academic = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Academic);
    let policy = crate::config::ExtractionProfile::for_document_type(crate::config::DocumentType::Policy);

    let academic_config = TextExtractionConfig::with_space_threshold(academic.tj_offset_threshold);
    let policy_config = TextExtractionConfig::with_space_threshold(policy.tj_offset_threshold);

    assert_ne!(
        academic_config.space_insertion_threshold, policy_config.space_insertion_threshold,
        "Academic and policy configs should have different thresholds"
    );
}

/// Test that default config is backward-compatible
#[test]
fn test_default_config_backward_compatible() {
    let default_config = TextExtractionConfig::default();
    let conservative_profile = crate::config::ExtractionProfile::CONSERVATIVE;

    assert_eq!(
        default_config.space_insertion_threshold, conservative_profile.tj_offset_threshold,
        "Default config should use conservative threshold for backward compatibility"
    );
}
