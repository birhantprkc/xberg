//! Tests for the image classification/clustering module, split out of `image_kind.rs`
//! to keep that file under the line-count limit.

use super::*;
#[cfg(any(feature = "ocr", feature = "ocr-wasm"))]
use image::{ImageBuffer, Rgba};

#[test]
fn test_classify_returns_mask_for_is_mask_true() {
    let (kind, conf) = classify(&[], "jpeg", Some(100), Some(100), None, None, true);
    assert_eq!(kind, ImageKind::Mask);
    assert_eq!(conf, 0.95);
}

#[test]
fn test_classify_returns_icon_for_small_square() {
    let (kind, conf) = classify(&[], "png", Some(48), Some(48), None, None, false);
    assert_eq!(kind, ImageKind::Icon);
    assert_eq!(conf, 0.85);
}

#[test]
fn test_classify_returns_decoration_for_tiny_strip() {
    let (kind, conf) = classify(&[], "png", Some(10), Some(100), None, None, false);
    assert_eq!(kind, ImageKind::Decoration);
    assert_eq!(conf, 0.80);
}

#[test]
fn test_classify_returns_textblock_for_gray_1bpp() {
    let (kind, conf) = classify(&[], "png", Some(200), Some(200), Some("Gray"), Some(1), false);
    assert_eq!(kind, ImageKind::TextBlock);
    assert_eq!(conf, 0.75);
}

#[test]
fn test_classify_returns_photograph_for_cmyk_8bpp() {
    let (kind, conf) = classify(&[], "jpeg", Some(800), Some(800), Some("CMYK"), Some(8), false);
    assert_eq!(kind, ImageKind::Photograph);
    assert_eq!(conf, 0.70);
}

#[test]
fn test_classify_returns_photograph_for_large_jpeg() {
    let (kind, conf) = classify(&[], "jpeg", Some(1000), Some(1000), None, None, false);
    assert_eq!(kind, ImageKind::Photograph);
    assert_eq!(conf, 0.85);
}

#[test]
fn test_classify_returns_diagram_for_flate_indexed() {
    let (kind, conf) = classify(&[], "flate", Some(200), Some(200), Some("Indexed"), None, false);
    assert_eq!(kind, ImageKind::Diagram);
    assert_eq!(conf, 0.65);
}

#[test]
fn test_classify_returns_mask_for_ccitt() {
    let (kind, conf) = classify(&[], "ccitt", Some(200), Some(200), None, None, false);
    assert_eq!(kind, ImageKind::Mask);
    assert_eq!(conf, 0.85);
}

#[cfg(any(feature = "ocr", feature = "ocr-wasm"))]
#[test]
fn test_classify_returns_photograph_for_high_entropy_thumbnail() {
    let mut state: u32 = 0x9E37_79B9;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state & 0xFF) as u8
    };
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(100, 100, |_x, _y| Rgba([next(), next(), next(), 255]));

    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();

    let (kind, conf) = classify(&bytes, "png", Some(100), Some(100), None, None, false);
    assert_eq!(kind, ImageKind::Photograph);
    assert!(conf >= 0.6, "confidence {} should be >= 0.6", conf);
}

#[cfg(any(feature = "ocr", feature = "ocr-wasm"))]
#[test]
fn should_reject_oversized_declared_dimensions_before_entropy_decode() {
    let oversized = crate::extraction::image_decode::bmp_with_declared_dimensions(6_000, 6_000);

    let error = compute_entropy_on_thumbnail(&oversized, 6_000, 6_000)
        .expect_err("oversized image must fail at the decoded-image budget");

    assert!(
        error.contains("security_limits.max_content_size"),
        "unexpected error: {error}"
    );
}

#[cfg(any(feature = "ocr", feature = "ocr-wasm"))]
#[test]
fn test_classify_returns_chart_for_low_entropy_small_image() {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(256, 256, |x, _y| {
        if x < 128 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });

    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();

    let (kind, conf) = classify(&bytes, "png", Some(256), Some(256), None, None, false);
    assert_eq!(kind, ImageKind::Chart);
    assert!(conf >= 0.55, "confidence {} should be >= 0.55", conf);
}

#[test]
fn test_classify_returns_unknown_for_truncated_bytes() {
    let truncated = vec![0x89, 0x50, 0x4E, 0x47];
    let (kind, conf) = classify(&truncated, "png", Some(100), Some(100), None, None, false);
    assert_eq!(kind, ImageKind::Unknown);
    assert_eq!(conf, 0.50);
}

#[test]
fn test_classify_never_panics_on_garbage_input() {
    let test_cases = vec![
        (&[][..], "unknown", Some(0u32), Some(0u32), None, None, false),
        (
            b"garbage",
            "jpeg",
            Some(1u32),
            Some(1u32),
            Some("RGB"),
            Some(8u32),
            false,
        ),
        (
            b"\xFF\xD8\xFF\xFF",
            "jpeg",
            Some(10000u32),
            Some(10000u32),
            None,
            None,
            false,
        ),
        (b"\x89PNG\r\n\x1a\n", "png", Some(0u32), Some(0u32), None, None, false),
        (
            b"",
            "unknown",
            Some(65536u32),
            Some(65536u32),
            Some("CMYK"),
            Some(16u32),
            true,
        ),
    ];

    for (bytes, fmt, w, h, cs, bpc, is_mask) in test_cases {
        let _ = classify(bytes, fmt, w, h, cs, bpc, is_mask);
    }
}

#[test]
fn test_cluster_tiles_groups_adjacent_similar_tiles() {
    let mut images = vec![
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 0,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: Some(crate::types::BoundingBox {
                x0: 0.0,
                y0: 0.0,
                x1: 100.0,
                y1: 100.0,
            }),
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 1,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: Some(crate::types::BoundingBox {
                x0: 101.0,
                y0: 0.0,
                x1: 201.0,
                y1: 100.0,
            }),
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
    ];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, Some(1));
    assert_eq!(images[1].cluster_id, Some(1));
    assert_eq!(images[0].image_kind, Some(ImageKind::TileFragment));
    assert_eq!(images[1].image_kind, Some(ImageKind::TileFragment));
}

#[test]
fn test_cluster_tiles_keeps_singletons_unclustered() {
    let mut images = vec![ExtractedImage {
        data: bytes::Bytes::new(),
        format: "png".into(),
        image_index: 0,
        page_number: Some(1),
        width: Some(100),
        height: Some(100),
        colorspace: None,
        bits_per_component: None,
        is_mask: false,
        description: None,
        ocr_result: None,
        bounding_box: None,
        source_path: None,
        image_kind: Some(ImageKind::Photograph),
        kind_confidence: Some(0.8),
        cluster_id: None,
        caption: None,
        qr_codes: None,
        data_base64: None,
    }];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, None);
    assert_eq!(images[0].image_kind, Some(ImageKind::Photograph));
}

#[test]
fn test_cluster_tiles_separates_distant_tiles() {
    let mut images = vec![
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 0,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: Some(crate::types::BoundingBox {
                x0: 0.0,
                y0: 0.0,
                x1: 100.0,
                y1: 100.0,
            }),
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 1,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: Some(crate::types::BoundingBox {
                x0: 500.0,
                y0: 500.0,
                x1: 600.0,
                y1: 600.0,
            }),
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
    ];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, None);
    assert_eq!(images[1].cluster_id, None);
}

#[test]
fn test_cluster_tiles_separates_dissimilar_kinds() {
    let mut images = vec![
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 0,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: None,
            source_path: None,
            image_kind: Some(ImageKind::Photograph),
            kind_confidence: Some(0.8),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 1,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: None,
            source_path: None,
            image_kind: Some(ImageKind::Photograph),
            kind_confidence: Some(0.8),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
    ];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, None);
    assert_eq!(images[1].cluster_id, None);
}

#[test]
fn test_cluster_tiles_falls_back_when_bounding_boxes_missing() {
    let mut images = vec![
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 0,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: None,
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
        ExtractedImage {
            data: bytes::Bytes::new(),
            format: "png".into(),
            image_index: 1,
            page_number: Some(1),
            width: Some(100),
            height: Some(100),
            colorspace: None,
            bits_per_component: None,
            is_mask: false,
            description: None,
            ocr_result: None,
            bounding_box: None,
            source_path: None,
            image_kind: Some(ImageKind::Drawing),
            kind_confidence: Some(0.7),
            cluster_id: None,
            caption: None,
            qr_codes: None,
            data_base64: None,
        },
    ];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, Some(1));
    assert_eq!(images[1].cluster_id, Some(1));
}

/// Build a 100x100 page-1 test image at the given `bounding_box`, matching every field the
/// verbose literals in `test_cluster_tiles_assigns_unique_ids` used to spell out by hand;
/// every other field keeps its `ExtractedImage::default()` value (`None`/`false`/empty),
/// same as those literals did explicitly. ~keep
fn unique_id_test_tile(
    index: u32,
    bounding_box: crate::types::BoundingBox,
    kind: ImageKind,
    confidence: f32,
) -> ExtractedImage {
    ExtractedImage {
        format: "png".into(),
        image_index: index,
        page_number: Some(1),
        width: Some(100),
        height: Some(100),
        bounding_box: Some(bounding_box),
        image_kind: Some(kind),
        kind_confidence: Some(confidence),
        ..Default::default()
    }
}

#[test]
fn test_cluster_tiles_assigns_unique_ids() {
    let mut images = vec![
        unique_id_test_tile(
            0,
            crate::types::BoundingBox {
                x0: 0.0,
                y0: 0.0,
                x1: 100.0,
                y1: 100.0,
            },
            ImageKind::Drawing,
            0.7,
        ),
        unique_id_test_tile(
            1,
            crate::types::BoundingBox {
                x0: 101.0,
                y0: 0.0,
                x1: 201.0,
                y1: 100.0,
            },
            ImageKind::Drawing,
            0.7,
        ),
        unique_id_test_tile(
            2,
            crate::types::BoundingBox {
                x0: 0.0,
                y0: 200.0,
                x1: 100.0,
                y1: 300.0,
            },
            ImageKind::Diagram,
            0.65,
        ),
        unique_id_test_tile(
            3,
            crate::types::BoundingBox {
                x0: 101.0,
                y0: 200.0,
                x1: 201.0,
                y1: 300.0,
            },
            ImageKind::Diagram,
            0.65,
        ),
    ];

    cluster_tiles(&mut images);

    assert_eq!(images[0].cluster_id, Some(1));
    assert_eq!(images[1].cluster_id, Some(1));
    assert_eq!(images[2].cluster_id, Some(2));
    assert_eq!(images[3].cluster_id, Some(2));
}

#[test]
fn test_cluster_tiles_is_deterministic() {
    let make_images = || {
        vec![
            ExtractedImage {
                data: bytes::Bytes::new(),
                format: "png".into(),
                image_index: 0,
                page_number: Some(1),
                width: Some(100),
                height: Some(100),
                colorspace: None,
                bits_per_component: None,
                is_mask: false,
                description: None,
                ocr_result: None,
                bounding_box: None,
                source_path: None,
                image_kind: Some(ImageKind::Drawing),
                kind_confidence: Some(0.7),
                cluster_id: None,
                caption: None,
                qr_codes: None,
                data_base64: None,
            },
            ExtractedImage {
                data: bytes::Bytes::new(),
                format: "png".into(),
                image_index: 1,
                page_number: Some(1),
                width: Some(100),
                height: Some(100),
                colorspace: None,
                bits_per_component: None,
                is_mask: false,
                description: None,
                ocr_result: None,
                bounding_box: None,
                source_path: None,
                image_kind: Some(ImageKind::Drawing),
                kind_confidence: Some(0.7),
                cluster_id: None,
                caption: None,
                qr_codes: None,
                data_base64: None,
            },
        ]
    };

    let mut images1 = make_images();
    let mut images2 = make_images();

    cluster_tiles(&mut images1);
    cluster_tiles(&mut images2);

    assert_eq!(images1[0].cluster_id, images2[0].cluster_id);
    assert_eq!(images1[1].cluster_id, images2[1].cluster_id);
}

#[test]
fn test_classify_skips_entropy_for_oversized_image() {
    let bytes = b"\x89PNG\r\n\x1a\nbogus body".to_vec();
    let (kind, conf) = classify(&bytes, "png", Some(20_000), Some(20_000), None, None, false);
    assert_eq!(kind, ImageKind::Unknown);
    assert_eq!(conf, 0.50);
}

#[test]
fn test_cluster_tiles_isolates_clusters_per_page() {
    let mut images = vec![];
    for page in 1..=2 {
        for col in 0..2 {
            images.push(ExtractedImage {
                data: bytes::Bytes::new(),
                format: "png".into(),
                image_index: ((page - 1) * 2 + col),
                page_number: Some(page),
                width: Some(100),
                height: Some(100),
                colorspace: None,
                bits_per_component: None,
                is_mask: false,
                description: None,
                ocr_result: None,
                bounding_box: Some(crate::types::BoundingBox {
                    x0: (col as f64) * 101.0,
                    y0: 0.0,
                    x1: (col as f64) * 101.0 + 100.0,
                    y1: 100.0,
                }),
                source_path: None,
                image_kind: Some(ImageKind::Drawing),
                kind_confidence: Some(0.7),
                cluster_id: None,
                caption: None,
                qr_codes: None,
                data_base64: None,
            });
        }
    }
    cluster_tiles(&mut images);
    assert!(images[0].cluster_id.is_some());
    assert_eq!(images[0].cluster_id, images[1].cluster_id);
    assert_eq!(images[2].cluster_id, images[3].cluster_id);
    assert_ne!(images[0].cluster_id, images[2].cluster_id);
}

#[test]
fn test_classify_does_not_panic_on_zero_dimensions() {
    let bytes = b"\x89PNG\r\n\x1a\nbody".to_vec();
    let (kind, conf) = classify(&bytes, "png", Some(0), Some(0), None, None, false);
    assert_eq!(kind, ImageKind::Unknown);
    assert_eq!(conf, 0.0);
}

#[test]
fn test_image_kind_serde_round_trips_all_variants() {
    let variants = [
        (ImageKind::Photograph, "photograph"),
        (ImageKind::Diagram, "diagram"),
        (ImageKind::Chart, "chart"),
        (ImageKind::Drawing, "drawing"),
        (ImageKind::TextBlock, "text_block"),
        (ImageKind::Decoration, "decoration"),
        (ImageKind::Logo, "logo"),
        (ImageKind::Icon, "icon"),
        (ImageKind::TileFragment, "tile_fragment"),
        (ImageKind::Mask, "mask"),
        (ImageKind::Unknown, "unknown"),
    ];
    for (kind, expected) in variants {
        let json = serde_json::to_string(&kind).expect("serialize");
        assert_eq!(json, format!("\"{expected}\""), "wrong wire name for {kind:?}");
        let round_trip: ImageKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_trip, kind);
    }
}
