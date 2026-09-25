use super::*;

#[test]
fn test_ascii_space_detection() {
    let characters = vec![
        CharacterInfo {
            code: 0x48,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x65,
            glyph_id: Some(2),
            width: 0.4,
            x_position: 6.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x20,
            glyph_id: Some(5),
            width: 0.25,
            x_position: 10.8,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x57,
            glyph_id: Some(6),
            width: 0.7,
            x_position: 16.2,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(boundaries.contains(&3));
}

#[test]
fn test_tj_offset_threshold() {
    let characters = vec![
        CharacterInfo {
            code: 0x54,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x2D,
            glyph_id: Some(5),
            width: 0.25,
            x_position: 6.0,
            tj_offset: Some(-200),
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x6F,
            glyph_id: Some(6),
            width: 0.4,
            x_position: 18.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(boundaries.contains(&2));
}

#[test]
fn test_geometric_gap_detection() {
    let characters = vec![
        CharacterInfo {
            code: 0x54,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x65,
            glyph_id: Some(2),
            width: 0.4,
            x_position: 6.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x78,
            glyph_id: Some(3),
            width: 0.4,
            x_position: 10.8,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x74,
            glyph_id: Some(4),
            width: 0.3,
            x_position: 15.6,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        // Gap of ~11.1 units (much larger than threshold ~3.6) ~keep
        CharacterInfo {
            code: 0x42,
            glyph_id: Some(5),
            width: 0.5,
            x_position: 27.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    // Gap between 't' (ends at 15.9) and 'B' (at 27.0) is 11.1 units > threshold (3.6)
    // This creates a boundary at index 4 (the 'B' character) ~keep
    assert!(
        boundaries.contains(&4),
        "Expected boundary at index 4, got: {:?}",
        boundaries
    );
}

#[test]
fn test_cjk_character_boundaries() {
    let characters = vec![
        CharacterInfo {
            code: 0x4E2D,
            glyph_id: Some(1),
            width: 1.0,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x6587,
            glyph_id: Some(2),
            width: 1.0,
            x_position: 12.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x5B57,
            glyph_id: Some(3),
            width: 1.0,
            x_position: 24.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    // Each CJK character creates a boundary after it
    // Character 0 -> boundary at 1, Character 1 -> boundary at 2 ~keep
    assert!(boundaries.contains(&1), "Expected boundary at index 1");
    assert!(boundaries.contains(&2), "Expected boundary at index 2");
}

#[test]
fn test_zero_width_space() {
    let characters = vec![
        CharacterInfo {
            code: 0x6E,
            glyph_id: Some(1),
            width: 0.4,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x200B,
            glyph_id: Some(2),
            width: 0.0,
            x_position: 4.8,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x72,
            glyph_id: Some(3),
            width: 0.3,
            x_position: 4.8,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(boundaries.contains(&2));
}

#[test]
fn test_horizontal_scaling_affects_gap_threshold() {
    // Create a gap that's on the threshold boundary
    // Gap = 7.5 units
    // At 100% scaling (font size 12): threshold = 12 * 0.8 = 9.6, gap < threshold = no boundary
    // At 75% scaling (font size 9): threshold = 9 * 0.8 = 7.2, gap > threshold = boundary!
    // ~keep
    let characters = vec![
        CharacterInfo {
            code: 0x41,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x42,
            glyph_id: Some(2),
            width: 0.5,
            x_position: 8.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    // With 100% scaling, gap (7.5) doesn't exceed threshold (9.6) ~keep
    let mut context = BoundaryContext::new(12.0);
    context.horizontal_scaling = 100.0;
    let boundaries_normal = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    // With 75% scaling, gap (7.5) exceeds threshold (7.2) ~keep
    context.horizontal_scaling = 75.0;
    let boundaries_scaled = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    // Scaling affects the effective threshold, so results should differ
    // Normal: no boundary, Scaled: boundary at index 1 ~keep
    assert!(
        boundaries_normal.is_empty(),
        "Should have no boundaries at 100% scaling"
    );
    assert!(boundaries_scaled.contains(&1), "Should have boundary at 75% scaling");
}

#[test]
fn test_detect_word_boundaries_ascii_space() {
    let characters = vec![
        CharacterInfo {
            code: 0x48,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x65,
            glyph_id: Some(2),
            width: 0.4,
            x_position: 6.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x20,
            glyph_id: Some(5),
            width: 0.25,
            x_position: 10.8,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x57,
            glyph_id: Some(6),
            width: 0.7,
            x_position: 16.2,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(boundaries.contains(&3), "Should have boundary after space");
}

#[test]
fn test_detect_word_boundaries_tj_offset() {
    let characters = vec![
        CharacterInfo {
            code: 0x54,
            glyph_id: Some(1),
            width: 0.5,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x2D,
            glyph_id: Some(5),
            width: 0.25,
            x_position: 6.0,
            tj_offset: Some(-200),
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x6F,
            glyph_id: Some(6),
            width: 0.4,
            x_position: 18.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(boundaries.contains(&2), "Should have boundary after large TJ offset");
}

#[test]
fn test_detect_word_boundaries_cjk() {
    let characters = vec![
        CharacterInfo {
            code: 0x4E2D,
            glyph_id: Some(1),
            width: 1.0,
            x_position: 0.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
        CharacterInfo {
            code: 0x6587,
            glyph_id: Some(2),
            width: 1.0,
            x_position: 12.0,
            tj_offset: None,
            font_size: 12.0,
            is_ligature: false,
            original_ligature: None,
            protected_from_split: false,
        },
    ];

    let context = BoundaryContext::new(12.0);
    let boundaries = WordBoundaryDetector::new().detect_word_boundaries(&characters, &context);

    assert!(
        boundaries.contains(&1),
        "Should have boundary after first CJK character"
    );
}

#[test]
fn test_calculate_tj_threshold_default_font() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(12.0);
    let threshold = detector.calculate_tj_threshold(&context);
    // Expected: -12.0 * 1.0 * 0.025 = -0.3 ~keep
    assert!(
        (threshold - (-0.3)).abs() < 0.01,
        "12pt font should give -0.3, got {}",
        threshold
    );
}

#[test]
fn test_calculate_tj_threshold_large_font() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(24.0);
    let threshold = detector.calculate_tj_threshold(&context);
    // Expected: -24.0 * 1.0 * 0.025 = -0.6 ~keep
    assert!(
        (threshold - (-0.6)).abs() < 0.01,
        "24pt font should give -0.6, got {}",
        threshold
    );
}

#[test]
fn test_calculate_tj_threshold_with_char_spacing() {
    let detector = WordBoundaryDetector::new();
    let mut context = BoundaryContext::new(12.0);
    context.char_spacing = 2.0;
    let threshold = detector.calculate_tj_threshold(&context);
    // Expected: -0.3 - (2.0 * 0.5) = -1.3 ~keep
    assert!(
        (threshold - (-1.3)).abs() < 0.01,
        "With char_spacing=2.0, expected -1.3, got {}",
        threshold
    );
}

#[test]
fn test_calculate_tj_threshold_with_word_spacing() {
    let detector = WordBoundaryDetector::new();
    let mut context = BoundaryContext::new(12.0);
    context.word_spacing = 3.0;
    let threshold = detector.calculate_tj_threshold(&context);
    // Expected: -0.3 - (3.0 * 0.5) = -1.8 ~keep
    assert!(
        (threshold - (-1.8)).abs() < 0.01,
        "With word_spacing=3.0, expected -1.8, got {}",
        threshold
    );
}

#[test]
fn test_calculate_tj_threshold_with_horizontal_scaling() {
    let detector = WordBoundaryDetector::new();
    let mut context = BoundaryContext::new(12.0);
    context.horizontal_scaling = 80.0;
    let threshold = detector.calculate_tj_threshold(&context);
    // Expected: -12.0 * 0.8 * 0.025 = -0.24 ~keep
    assert!(
        (threshold - (-0.24)).abs() < 0.01,
        "With 80% scaling, expected -0.24, got {}",
        threshold
    );
}

#[test]
fn test_adaptive_threshold_affects_boundary_detection() {
    let detector = WordBoundaryDetector::new().with_adaptive_threshold(true);
    let context = BoundaryContext::new(12.0);

    let prev = CharacterInfo {
        code: 't' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: Some(-200),
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    let curr = CharacterInfo {
        code: 'h' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 110.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    let boundary = detector.is_word_boundary(&prev, &curr, &context);
    assert!(boundary, "TJ offset -200 should trigger boundary with 12pt font");
}

#[test]
fn test_disable_adaptive_threshold_uses_static() {
    let detector = WordBoundaryDetector::new()
        .with_adaptive_threshold(false)
        .with_tj_threshold(-100);
    let context = BoundaryContext::new(12.0);

    let prev = CharacterInfo {
        code: 'a' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: Some(-50),
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    let curr = CharacterInfo {
        code: 'b' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 110.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    let boundary = detector.is_word_boundary(&prev, &curr, &context);
    assert!(
        !boundary,
        "TJ offset -50 should NOT trigger boundary when static -100 is used"
    );
}

#[test]
fn test_geometric_gap_basic() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(12.0);

    let prev = CharacterInfo {
        code: 't' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    // Large gap (10 units > 9.6 = 12*0.8 threshold) ~keep
    let curr = CharacterInfo {
        code: 'h' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 115.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    assert!(
        detector.has_significant_geometric_gap(&prev, &curr, &context),
        "Gap of 10 units should exceed threshold of 9.6 (12pt * 0.8)"
    );
}

#[test]
fn test_geometric_gap_with_char_spacing() {
    let detector = WordBoundaryDetector::new();
    let mut context = BoundaryContext::new(12.0);
    context.char_spacing = 2.0;

    let prev = CharacterInfo {
        code: 'a' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    // Raw gap = 10, but Tc = 2.0 reduces it to 8.0
    // 8.0 < 9.6 (threshold), so NO boundary ~keep
    let curr = CharacterInfo {
        code: 'b' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 115.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    assert!(
        !detector.has_significant_geometric_gap(&prev, &curr, &context),
        "Gap of 10 - 2.0 (Tc) = 8.0 should NOT exceed threshold of 9.6"
    );
}

#[test]
fn test_ligature_internal_gap_fi() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(12.0);

    // 'f' component from expanded 'fi' ligature ~keep
    let prev = CharacterInfo {
        code: 'f' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: true,
        original_ligature: Some('ﬁ'),
        protected_from_split: false,
    };

    // Large gap but prev is from ligature ~keep
    let curr = CharacterInfo {
        code: 'i' as u32,
        glyph_id: None,
        width: 3.0,
        x_position: 120.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: true,
        original_ligature: Some('ﬁ'),
        protected_from_split: false,
    };

    assert!(
        !detector.has_significant_geometric_gap(&prev, &curr, &context),
        "Ligature internal gap should NOT create boundary even with large gap"
    );
}

#[test]
fn test_punctuation_reduced_threshold() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(12.0);
    // Base threshold: 12.0 * 0.8 = 9.6
    // Punctuation threshold: 9.6 * 0.5 = 4.8 ~keep

    let prev = CharacterInfo {
        code: 'd' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    // Gap of 6.0 units
    // Normal threshold would be 9.6 (no boundary)
    // Punctuation threshold is 4.8 (YES boundary) ~keep
    let curr_period = CharacterInfo {
        code: '.' as u32,
        glyph_id: None,
        width: 2.0,
        x_position: 111.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    assert!(
        detector.has_significant_geometric_gap(&prev, &curr_period, &context),
        "Gap of 6 units should exceed punctuation threshold of 4.8 (50% of 9.6)"
    );
}

#[test]
fn test_punctuation_does_not_trigger_on_normal_text() {
    let detector = WordBoundaryDetector::new();
    let context = BoundaryContext::new(12.0);

    let prev = CharacterInfo {
        code: 'd' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    // Same gap (6.0) but current character is 'e', not punctuation ~keep
    let curr = CharacterInfo {
        code: 'e' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 111.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    assert!(
        !detector.has_significant_geometric_gap(&prev, &curr, &context),
        "Gap of 6 units should NOT exceed normal threshold of 9.6"
    );
}

#[test]
fn test_is_punctuation_ascii() {
    assert!(WordBoundaryDetector::is_punctuation('.' as u32));
    assert!(WordBoundaryDetector::is_punctuation(',' as u32));
    assert!(WordBoundaryDetector::is_punctuation('!' as u32));
    assert!(WordBoundaryDetector::is_punctuation('?' as u32));
    assert!(WordBoundaryDetector::is_punctuation(':' as u32));
    assert!(WordBoundaryDetector::is_punctuation(';' as u32));
}

#[test]
fn test_is_punctuation_non_punctuation() {
    assert!(!WordBoundaryDetector::is_punctuation('a' as u32));
    assert!(!WordBoundaryDetector::is_punctuation('1' as u32));
    assert!(!WordBoundaryDetector::is_punctuation(' ' as u32));
}

#[test]
fn test_is_ligature_internal_gap_ffi() {
    let detector = WordBoundaryDetector::new();

    // 'f' from 'ffi' ligature ~keep
    let prev = CharacterInfo {
        code: 'f' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: true,
        original_ligature: Some('ﬄ'),
        protected_from_split: false,
    };

    let curr = CharacterInfo {
        code: 'f' as u32,
        glyph_id: None,
        width: 5.0,
        x_position: 110.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: true,
        original_ligature: Some('ﬄ'),
        protected_from_split: false,
    };

    assert!(
        detector.is_ligature_internal_gap(&prev, &curr),
        "Should detect ligature internal gap when both have is_ligature=true"
    );
}

#[test]
fn test_is_ligature_internal_gap_actual_ligature_code() {
    let detector = WordBoundaryDetector::new();

    // Previous character IS the ligature U+FB00 ('ff') ~keep
    let prev = CharacterInfo {
        code: 0xFB00,
        glyph_id: None,
        width: 10.0,
        x_position: 100.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    let curr = CharacterInfo {
        code: 'i' as u32,
        glyph_id: None,
        width: 3.0,
        x_position: 115.0,
        tj_offset: None,
        font_size: 12.0,
        is_ligature: false,
        original_ligature: None,
        protected_from_split: false,
    };

    assert!(
        detector.is_ligature_internal_gap(&prev, &curr),
        "Should detect ligature internal gap when prev code is U+FB00"
    );
}
