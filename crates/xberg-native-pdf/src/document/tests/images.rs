use super::super::*;
use super::common::*;

/// Regression test: circular Form XObject references must not cause
/// a stack overflow / segfault. The PDF has X0→X1→X0 circular references.
#[test]
fn test_issue_163_circular_form_xobjects() {
    let pdf_bytes = build_circular_xobject_pdf();
    let dir = tempfile::tempdir().expect("create temp dir");
    let tmp_path = dir.path().join("native_pdf_test_issue163.pdf");
    std::fs::write(&tmp_path, &pdf_bytes).unwrap();
    let doc = PdfDocument::open(&tmp_path).unwrap();
    let _ = std::fs::remove_file(&tmp_path);
    assert_eq!(doc.page_count().unwrap(), 1);

    let text = doc.extract_text(0).unwrap();
    assert!(text.is_empty() || text.len() < 100);

    // extract_images should not hang or crash (this was the segfault path) ~keep
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());

    let text = doc.extract_text(0).unwrap();
    drop(text);
}

#[test]
fn test_find_references_reference() {
    let obj = Object::Reference(ObjectRef::new(5, 0));
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0], ObjectRef::new(5, 0));
}

#[test]
fn test_find_references_array() {
    let arr = Object::Array(vec![
        Object::Reference(ObjectRef::new(1, 0)),
        Object::Integer(42),
        Object::Reference(ObjectRef::new(2, 0)),
    ]);
    let refs = PdfDocument::find_references(&arr);
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_find_references_dictionary() {
    let mut dict = std::collections::HashMap::new();
    dict.insert("Key1".to_string(), Object::Reference(ObjectRef::new(3, 0)));
    dict.insert("Key2".to_string(), Object::Integer(1));
    let obj = Object::Dictionary(dict);
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_find_references_stream() {
    let mut dict = std::collections::HashMap::new();
    dict.insert("Length".to_string(), Object::Reference(ObjectRef::new(10, 0)));
    let obj = Object::Stream {
        dict,
        data: bytes::Bytes::from_static(b""),
    };
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_find_references_integer() {
    let refs = PdfDocument::find_references(&Object::Integer(42));
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_null() {
    let refs = PdfDocument::find_references(&Object::Null);
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_boolean() {
    let refs = PdfDocument::find_references(&Object::Boolean(true));
    assert!(refs.is_empty());
}

#[test]
fn test_find_references_nested() {
    let inner = Object::Array(vec![Object::Reference(ObjectRef::new(7, 0))]);
    let mut dict = std::collections::HashMap::new();
    dict.insert("Inner".to_string(), inner);
    dict.insert("Direct".to_string(), Object::Reference(ObjectRef::new(8, 0)));
    let obj = Object::Dictionary(dict);
    let refs = PdfDocument::find_references(&obj);
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_parse_matrix_from_object_valid() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Real(1.0),
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(1.0),
        Object::Real(10.0),
        Object::Real(20.0),
    ]);
    let matrix = doc.parse_matrix_from_object(&arr).unwrap();
    assert!((matrix.a - 1.0).abs() < f32::EPSILON);
    assert!((matrix.e - 10.0).abs() < f32::EPSILON);
    assert!((matrix.f - 20.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_matrix_from_object_integers() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Integer(2),
        Object::Integer(0),
        Object::Integer(0),
        Object::Integer(3),
        Object::Integer(100),
        Object::Integer(200),
    ]);
    let matrix = doc.parse_matrix_from_object(&arr).unwrap();
    assert!((matrix.a - 2.0).abs() < f32::EPSILON);
    assert!((matrix.d - 3.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_matrix_from_object_too_short() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![Object::Real(1.0), Object::Real(0.0)]);
    let result = doc.parse_matrix_from_object(&arr);
    assert!(result.is_none());
}

#[test]
fn test_parse_matrix_from_object_not_array() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let result = doc.parse_matrix_from_object(&Object::Integer(42));
    assert!(result.is_none());
}

#[test]
fn test_parse_matrix_from_object_invalid_elements() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let arr = Object::Array(vec![
        Object::Real(1.0),
        Object::Name("bad".to_string()),
        Object::Real(0.0),
        Object::Real(1.0),
        Object::Real(0.0),
        Object::Real(0.0),
    ]);
    let result = doc.parse_matrix_from_object(&arr);
    assert!(result.is_none());
}

#[test]
fn test_transform_bbox_identity() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix::identity();
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.x - 10.0).abs() < f32::EPSILON);
    assert!((result.y - 20.0).abs() < f32::EPSILON);
    assert!((result.width - 100.0).abs() < f32::EPSILON);
    assert!((result.height - 50.0).abs() < f32::EPSILON);
}

#[test]
fn test_transform_bbox_translation() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 50.0,
        f: 100.0,
    };
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.x - 50.0).abs() < f32::EPSILON);
    assert!((result.y - 100.0).abs() < f32::EPSILON);
}

#[test]
fn test_transform_bbox_scaling() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();

    let rect = crate::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    let ctm = crate::content::Matrix {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 3.0,
        e: 0.0,
        f: 0.0,
    };
    let result = doc.transform_bbox_with_ctm(&rect, ctm);
    assert!((result.width - 200.0).abs() < f32::EPSILON);
    assert!((result.height - 150.0).abs() < f32::EPSILON);
}

#[test]
fn test_check_for_circular_references_runs() {
    // Minimal PDFs naturally have Page <-> Pages parent references,
    // so we just verify the function runs without panicking
    // returns a list (which may include the Page<->Pages backreference). ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let cycles = doc.check_for_circular_references();
    let _ = cycles;
}

#[test]
fn test_extract_images_blank_page() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());
}

#[test]
fn test_extract_images_graphics_only() {
    let content = b"100 200 300 400 re S";
    let pdf = build_minimal_pdf(content);
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    assert!(images.is_empty());
}

#[test]
fn test_extracted_image_ref_debug() {
    let img_ref = ExtractedImageRef {
        filename: "img_001.png".to_string(),
        format: ImageFormat::Png,
        width: 100,
        height: 200,
        bbox: None,
        rotation: 0,
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    let debug = format!("{:?}", img_ref);
    assert!(debug.contains("img_001.png"));
    assert!(debug.contains("Png"));
}

#[test]
fn test_extracted_image_ref_clone() {
    let img_ref = ExtractedImageRef {
        filename: "img_001.jpg".to_string(),
        format: ImageFormat::Jpeg,
        width: 100,
        height: 200,
        bbox: None,
        rotation: 0,
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    let cloned = img_ref.clone();
    assert_eq!(img_ref, cloned);
}

#[test]
fn test_image_format_equality() {
    assert_eq!(ImageFormat::Png, ImageFormat::Png);
    assert_eq!(ImageFormat::Jpeg, ImageFormat::Jpeg);
    assert_ne!(ImageFormat::Png, ImageFormat::Jpeg);
}

#[test]
fn test_get_page_for_debug() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let page = doc.get_page_for_debug(0).unwrap();
    assert!(page.as_dict().is_some());
}

#[test]
fn test_may_contain_text_public() {
    assert!(PdfDocument::may_contain_text_public(b"BT /F1 12 Tf ET"));
    assert!(!PdfDocument::may_contain_text_public(b"100 200 re S"));
}

#[test]
fn test_find_references_string_obj() {
    assert!(PdfDocument::find_references(&Object::String(b"hello".to_vec())).is_empty());
}

#[test]
fn test_find_references_real_obj() {
    assert!(PdfDocument::find_references(&Object::Real(std::f64::consts::PI)).is_empty());
}

#[test]
fn test_find_references_name_obj() {
    assert!(PdfDocument::find_references(&Object::Name("Test".to_string())).is_empty());
}

#[test]
fn test_find_references_deeply_nested() {
    let inner_ref = Object::Reference(ObjectRef::new(10, 0));
    let inner_arr = Object::Array(vec![inner_ref]);
    let mut dict = std::collections::HashMap::new();
    dict.insert("Key".to_string(), inner_arr);
    let refs = PdfDocument::find_references(&Object::Dictionary(dict));
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, 10);
}

#[test]
fn test_check_circular_refs_on_minimal_pdf() {
    // The minimal PDF has a page tree cycle:
    // Pages (2 0 R) -> Kids -> Page (3 0 R) -> Parent -> Pages (2 0 R)
    // The DFS cycle detector reports this as a cycle. ~keep
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    let cycles = doc.check_for_circular_references();
    assert!(!cycles.is_empty());
}

#[test]
fn test_extract_images_out_of_bounds() {
    let pdf = build_minimal_pdf(b"");
    let doc = PdfDocument::from_bytes(pdf).unwrap();
    assert!(doc.extract_images(999).is_err());
}

#[test]
fn test_image_format_debug() {
    assert_eq!(format!("{:?}", ImageFormat::Png), "Png");
    assert_eq!(format!("{:?}", ImageFormat::Jpeg), "Jpeg");
}
