use super::*;
use crate::object::Object;
use std::collections::HashMap;

#[test]
fn test_algorithm_key_length_none() {
    assert_eq!(Algorithm::None.key_length(), 0);
}

#[test]
fn test_algorithm_key_length_rc4_40() {
    assert_eq!(Algorithm::RC4_40.key_length(), 5);
}

#[test]
fn test_algorithm_key_length_rc4_128() {
    assert_eq!(Algorithm::Rc4_128.key_length(), 16);
}

#[test]
fn test_algorithm_key_length_aes128() {
    assert_eq!(Algorithm::Aes128.key_length(), 16);
}

#[test]
fn test_algorithm_key_length_aes256() {
    assert_eq!(Algorithm::Aes256.key_length(), 32);
}

#[test]
fn test_algorithm_is_aes() {
    assert!(!Algorithm::None.is_aes());
    assert!(!Algorithm::RC4_40.is_aes());
    assert!(!Algorithm::Rc4_128.is_aes());
    assert!(Algorithm::Aes128.is_aes());
    assert!(Algorithm::Aes256.is_aes());
}

#[test]
fn test_algorithm_is_rc4() {
    assert!(!Algorithm::None.is_rc4());
    assert!(Algorithm::RC4_40.is_rc4());
    assert!(Algorithm::Rc4_128.is_rc4());
    assert!(!Algorithm::Aes128.is_rc4());
    assert!(!Algorithm::Aes256.is_rc4());
}

#[test]
fn test_algorithm_none_not_aes_not_rc4() {
    assert!(!Algorithm::None.is_aes());
    assert!(!Algorithm::None.is_rc4());
}

#[test]
fn test_algorithm_equality() {
    assert_eq!(Algorithm::RC4_40, Algorithm::RC4_40);
    assert_eq!(Algorithm::Aes256, Algorithm::Aes256);
    assert_ne!(Algorithm::RC4_40, Algorithm::Rc4_128);
    assert_ne!(Algorithm::Aes128, Algorithm::Aes256);
}

#[test]
fn test_algorithm_clone_copy() {
    let a = Algorithm::Aes128;
    let b = a;
    let c = a;
    assert_eq!(a, b);
    assert_eq!(a, c);
}

fn make_encrypt_dict_obj(filter: &str, v: i64, r: i64, owner: &[u8], user: &[u8], p: i64) -> Object {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name(filter.to_string()));
    dict.insert("V".to_string(), Object::Integer(v));
    dict.insert("R".to_string(), Object::Integer(r));
    dict.insert("O".to_string(), Object::String(owner.to_vec()));
    dict.insert("U".to_string(), Object::String(user.to_vec()));
    dict.insert("P".to_string(), Object::Integer(p));
    Object::Dictionary(dict)
}

#[test]
fn test_encrypt_dict_from_object_rc4_40() {
    let obj = make_encrypt_dict_obj("Standard", 1, 2, &[0u8; 32], &[0u8; 32], -4);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.filter, "Standard");
    assert_eq!(ed.version, 1);
    assert_eq!(ed.revision, 2);
    assert_eq!(ed.permissions, -4);
    assert!(ed.encrypt_metadata);
    assert!(ed.sub_filter.is_none());
    assert!(ed.length.is_none());
}

#[test]
fn test_encrypt_dict_from_object_rc4_128() {
    let obj = make_encrypt_dict_obj("Standard", 2, 3, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.version, 2);
    assert_eq!(ed.revision, 3);
}

#[test]
fn test_encrypt_dict_from_object_aes128() {
    let obj = make_encrypt_dict_obj("Standard", 4, 4, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.version, 4);
    assert_eq!(ed.revision, 4);
}

#[test]
fn test_encrypt_dict_from_object_aes256() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("V".to_string(), Object::Integer(5));
    dict.insert("R".to_string(), Object::Integer(6));
    dict.insert("O".to_string(), Object::String(vec![0u8; 48]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 48]));
    dict.insert("P".to_string(), Object::Integer(-1));
    dict.insert("OE".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("UE".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("Perms".to_string(), Object::String(vec![0u8; 16]));
    let obj = Object::Dictionary(dict);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.version, 5);
    assert_eq!(ed.revision, 6);
    assert!(ed.owner_encryption.is_some());
    assert!(ed.user_encryption.is_some());
    assert!(ed.perms.is_some());
}

#[test]
fn test_encrypt_dict_from_object_with_optional_fields() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("SubFilter".to_string(), Object::Name("adbe.pkcs7.s4".to_string()));
    dict.insert("V".to_string(), Object::Integer(2));
    dict.insert("R".to_string(), Object::Integer(3));
    dict.insert("Length".to_string(), Object::Integer(128));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-3904));
    dict.insert("EncryptMetadata".to_string(), Object::Boolean(false));
    let obj = Object::Dictionary(dict);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.sub_filter.as_deref(), Some("adbe.pkcs7.s4"));
    assert_eq!(ed.length, Some(128));
    assert!(!ed.encrypt_metadata);
}

#[test]
fn test_encrypt_dict_from_object_not_dict() {
    let obj = Object::Integer(42);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_filter() {
    let mut dict = HashMap::new();
    dict.insert("V".to_string(), Object::Integer(1));
    dict.insert("R".to_string(), Object::Integer(2));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-1));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_v() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("R".to_string(), Object::Integer(2));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-1));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_r() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("V".to_string(), Object::Integer(1));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-1));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_o() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("V".to_string(), Object::Integer(1));
    dict.insert("R".to_string(), Object::Integer(2));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-1));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_u() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("V".to_string(), Object::Integer(1));
    dict.insert("R".to_string(), Object::Integer(2));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("P".to_string(), Object::Integer(-1));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_encrypt_dict_from_object_missing_p() {
    let mut dict = HashMap::new();
    dict.insert("Filter".to_string(), Object::Name("Standard".to_string()));
    dict.insert("V".to_string(), Object::Integer(1));
    dict.insert("R".to_string(), Object::Integer(2));
    dict.insert("O".to_string(), Object::String(vec![0u8; 32]));
    dict.insert("U".to_string(), Object::String(vec![0u8; 32]));
    let obj = Object::Dictionary(dict);
    let result = EncryptDict::from_object(&obj);
    assert!(result.is_err());
}

#[test]
fn test_algorithm_detection_v1_r2() {
    let obj = make_encrypt_dict_obj("Standard", 1, 2, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::RC4_40);
}

#[test]
fn test_algorithm_detection_v2_r3() {
    let obj = make_encrypt_dict_obj("Standard", 2, 3, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Rc4_128);
}

#[test]
fn test_algorithm_detection_v4_r4() {
    let obj = make_encrypt_dict_obj("Standard", 4, 4, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Aes128);
}

#[test]
fn test_algorithm_detection_v5_r5() {
    let obj = make_encrypt_dict_obj("Standard", 5, 5, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Aes256);
}

#[test]
fn test_algorithm_detection_v5_r6() {
    let obj = make_encrypt_dict_obj("Standard", 5, 6, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Aes256);
}

#[test]
fn test_algorithm_detection_lenient_v1_r3() {
    // Non-standard R for V=1 should still return RC4_40 ~keep
    let obj = make_encrypt_dict_obj("Standard", 1, 3, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::RC4_40);
}

#[test]
fn test_algorithm_detection_lenient_v2_r4() {
    // Non-standard R for V=2 should still return Rc4_128 ~keep
    let obj = make_encrypt_dict_obj("Standard", 2, 4, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Rc4_128);
}

#[test]
fn test_algorithm_detection_lenient_v4_r5() {
    // Non-standard R for V=4 should still return Aes128 ~keep
    let obj = make_encrypt_dict_obj("Standard", 4, 5, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Aes128);
}

#[test]
fn test_algorithm_detection_unsupported() {
    let obj = make_encrypt_dict_obj("Standard", 99, 99, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    let result = ed.algorithm();
    assert!(result.is_err());
}

#[test]
fn test_key_length_bytes_with_length() {
    let obj = make_encrypt_dict_obj("Standard", 2, 3, &[0u8; 32], &[0u8; 32], -1);
    let mut ed = EncryptDict::from_object(&obj).unwrap();
    ed.length = Some(128);
    assert_eq!(ed.key_length_bytes(), 16);
}

#[test]
fn test_key_length_bytes_with_length_40() {
    let obj = make_encrypt_dict_obj("Standard", 1, 2, &[0u8; 32], &[0u8; 32], -1);
    let mut ed = EncryptDict::from_object(&obj).unwrap();
    ed.length = Some(40);
    assert_eq!(ed.key_length_bytes(), 5);
}

#[test]
fn test_key_length_bytes_default_v1() {
    let obj = make_encrypt_dict_obj("Standard", 1, 2, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.key_length_bytes(), 5);
}

#[test]
fn test_key_length_bytes_default_v2() {
    let obj = make_encrypt_dict_obj("Standard", 2, 3, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.key_length_bytes(), 16);
}

#[test]
fn test_key_length_bytes_default_v4() {
    let obj = make_encrypt_dict_obj("Standard", 4, 4, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.key_length_bytes(), 16);
}

#[test]
fn test_key_length_bytes_default_v5() {
    let obj = make_encrypt_dict_obj("Standard", 5, 6, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.key_length_bytes(), 32);
}

#[test]
fn test_key_length_bytes_default_unknown_v() {
    let obj = make_encrypt_dict_obj("Standard", 3, 3, &[0u8; 32], &[0u8; 32], -1);
    let ed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(ed.key_length_bytes(), 16);
}

#[test]
fn test_to_object_basic() {
    let ed = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 1,
        length: None,
        revision: 2,
        owner_password: vec![0u8; 32],
        user_password: vec![0u8; 32],
        permissions: -4,
        encrypt_metadata: true,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: None,
    };
    let obj = ed.to_object();
    let dict = obj.as_dict().unwrap();
    assert!(dict.contains_key("Filter"));
    assert!(dict.contains_key("V"));
    assert!(dict.contains_key("R"));
    assert!(dict.contains_key("O"));
    assert!(dict.contains_key("U"));
    assert!(dict.contains_key("P"));
    // EncryptMetadata is true (default), should NOT be written ~keep
    assert!(!dict.contains_key("EncryptMetadata"));
}

#[test]
fn test_to_object_with_sub_filter() {
    let ed = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: Some("adbe.pkcs7.s4".to_string()),
        version: 2,
        length: Some(128),
        revision: 3,
        owner_password: vec![0u8; 32],
        user_password: vec![0u8; 32],
        permissions: -1,
        encrypt_metadata: false,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: None,
    };
    let obj = ed.to_object();
    let dict = obj.as_dict().unwrap();
    assert!(dict.contains_key("SubFilter"));
    assert!(dict.contains_key("Length"));
    assert!(dict.contains_key("EncryptMetadata"));
}

#[test]
fn test_to_object_v4_has_crypt_filters() {
    let ed = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 4,
        length: None,
        revision: 4,
        owner_password: vec![0u8; 32],
        user_password: vec![0u8; 32],
        permissions: -1,
        encrypt_metadata: true,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: None,
    };
    let obj = ed.to_object();
    let dict = obj.as_dict().unwrap();
    assert!(dict.contains_key("CF"));
    assert!(dict.contains_key("StmF"));
    assert!(dict.contains_key("StrF"));
}

#[test]
fn test_to_object_v5_has_crypt_filters() {
    let ed = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 5,
        length: None,
        revision: 6,
        owner_password: vec![0u8; 48],
        user_password: vec![0u8; 48],
        permissions: -1,
        encrypt_metadata: true,
        owner_encryption: Some(vec![0u8; 32]),
        user_encryption: Some(vec![0u8; 32]),
        perms: Some(vec![0u8; 16]),
        stream_crypt_method: None,
    };
    let obj = ed.to_object();
    let dict = obj.as_dict().unwrap();
    assert!(dict.contains_key("CF"));
    assert!(dict.contains_key("StmF"));
    assert!(dict.contains_key("StrF"));
    assert!(dict.contains_key("OE"));
    assert!(dict.contains_key("UE"));
    assert!(dict.contains_key("Perms"));
}

#[test]
fn test_permissions_all_granted() {
    let perms = Permissions::from_bits(-1i32);
    assert!(perms.can_print());
    assert!(perms.can_modify());
    assert!(perms.can_copy());
    assert!(perms.can_annotate());
    assert!(perms.can_fill_forms());
    assert!(perms.can_extract_accessibility());
    assert!(perms.can_assemble());
    assert!(perms.can_print_high_quality());
}

#[test]
fn test_permissions_none_granted() {
    let perms = Permissions::from_bits(0);
    assert!(!perms.can_print());
    assert!(!perms.can_modify());
    assert!(!perms.can_copy());
    assert!(!perms.can_annotate());
    assert!(!perms.can_fill_forms());
    assert!(!perms.can_extract_accessibility());
    assert!(!perms.can_assemble());
    assert!(!perms.can_print_high_quality());
}

#[test]
fn test_permissions_print_only() {
    let perms = Permissions::from_bits(1 << 2);
    assert!(perms.can_print());
    assert!(!perms.can_modify());
    assert!(!perms.can_copy());
}

#[test]
fn test_permissions_modify_only() {
    let perms = Permissions::from_bits(1 << 3);
    assert!(!perms.can_print());
    assert!(perms.can_modify());
    assert!(!perms.can_copy());
}

#[test]
fn test_permissions_copy_only() {
    let perms = Permissions::from_bits(1 << 4);
    assert!(perms.can_copy());
    assert!(!perms.can_print());
    assert!(!perms.can_modify());
}

#[test]
fn test_permissions_annotate_only() {
    let perms = Permissions::from_bits(1 << 5);
    assert!(perms.can_annotate());
}

#[test]
fn test_permissions_fill_forms_only() {
    let perms = Permissions::from_bits(1 << 8);
    assert!(perms.can_fill_forms());
}

#[test]
fn test_permissions_extract_accessibility_only() {
    let perms = Permissions::from_bits(1 << 9);
    assert!(perms.can_extract_accessibility());
}

#[test]
fn test_permissions_assemble_only() {
    let perms = Permissions::from_bits(1 << 10);
    assert!(perms.can_assemble());
}

#[test]
fn test_permissions_high_quality_print_only() {
    let perms = Permissions::from_bits(1 << 11);
    assert!(perms.can_print_high_quality());
}

#[test]
fn test_permissions_combined() {
    // Print + Copy + Fill Forms ~keep
    let bits = (1 << 2) | (1 << 4) | (1 << 8);
    let perms = Permissions::from_bits(bits);
    assert!(perms.can_print());
    assert!(!perms.can_modify());
    assert!(perms.can_copy());
    assert!(!perms.can_annotate());
    assert!(perms.can_fill_forms());
}

#[test]
fn test_permissions_clone_copy() {
    let p = Permissions::from_bits(-1);
    let p2 = p;
    let p3 = p;
    assert!(p2.can_print());
    assert!(p3.can_print());
}

#[test]
fn test_builder_rc4_40() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::RC4_40)
        .user_password(b"user")
        .owner_password(b"owner")
        .permissions(-4)
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.filter, "Standard");
    assert_eq!(ed.version, 1);
    assert_eq!(ed.revision, 2);
    assert_eq!(ed.permissions, -4);
    assert_eq!(ed.length, Some(40));
    assert!(ed.encrypt_metadata);
}

#[test]
fn test_builder_rc4_128() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::Rc4_128)
        .user_password(b"pass")
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.version, 2);
    assert_eq!(ed.revision, 3);
    assert_eq!(ed.length, Some(128));
}

#[test]
fn test_builder_aes128() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::Aes128)
        .user_password(b"pass")
        .encrypt_metadata(false)
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.version, 4);
    assert_eq!(ed.revision, 4);
    assert!(!ed.encrypt_metadata);
}

#[test]
fn test_builder_aes256() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::Aes256)
        .user_password(b"user")
        .owner_password(b"owner")
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.version, 5);
    assert_eq!(ed.revision, 6);
    assert_eq!(ed.length, Some(256));
}

#[test]
fn test_builder_owner_password_defaults_to_user() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::RC4_40)
        .user_password(b"user_pass")
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.filter, "Standard");
    assert!(!ed.owner_password.is_empty());
}

#[test]
fn test_builder_default_permissions() {
    let file_id = vec![0u8; 16];
    let ed = EncryptDictBuilder::new(Algorithm::RC4_40)
        .user_password(b"pass")
        .build(&file_id)
        .unwrap();
    assert_eq!(ed.permissions, -1);
}

#[test]
fn test_generate_file_id_returns_16_bytes() {
    let (id1, id2) = generate_file_id();
    assert_eq!(id1.len(), 16);
    assert_eq!(id2.len(), 16);
}

#[test]
fn test_generate_file_id_both_equal() {
    let (id1, id2) = generate_file_id();
    assert_eq!(id1, id2);
}

#[test]
fn test_generate_file_id_unique() {
    let (id1, _) = generate_file_id();
    let (id2, _) = generate_file_id();
    // It's extremely unlikely these would be equal ~keep
    assert_ne!(id1, id2);
}

#[test]
fn test_generate_file_id_uniqueness() {
    let id1 = generate_file_id();
    let id2 = generate_file_id();
    assert_ne!(id1, id2, "file IDs must be unique across calls");
}

#[test]
fn test_encrypt_dict_roundtrip_v1() {
    let original = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 1,
        length: Some(40),
        revision: 2,
        owner_password: vec![1u8; 32],
        user_password: vec![2u8; 32],
        permissions: -4,
        encrypt_metadata: true,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: None,
    };
    let obj = original.to_object();
    let parsed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(parsed.filter, original.filter);
    assert_eq!(parsed.version, original.version);
    assert_eq!(parsed.revision, original.revision);
    assert_eq!(parsed.permissions, original.permissions);
    assert_eq!(parsed.owner_password, original.owner_password);
    assert_eq!(parsed.user_password, original.user_password);
}

#[test]
fn test_encrypt_dict_roundtrip_v4() {
    let original = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 4,
        length: None,
        revision: 4,
        owner_password: vec![3u8; 32],
        user_password: vec![4u8; 32],
        permissions: -1,
        encrypt_metadata: true,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: None,
    };
    let obj = original.to_object();
    let parsed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(parsed.version, 4);
    assert_eq!(parsed.revision, 4);
    assert_eq!(parsed.algorithm().unwrap(), Algorithm::Aes128);
    assert_eq!(parsed.stream_crypt_method.as_deref(), Some("AESV2"));
}

#[test]
fn test_v4_cfm_v2_selects_rc4_128() {
    // V=4 with CFM=V2 means RC4-128, NOT AES-128.
    // This is the exact case from issue #202 (OpenPDF 1.3.26). ~keep
    let ed = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 4,
        length: Some(128),
        revision: 4,
        owner_password: vec![0u8; 32],
        user_password: vec![0u8; 32],
        permissions: -1580,
        encrypt_metadata: false,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: Some("V2".to_string()),
    };
    assert_eq!(ed.algorithm().unwrap(), Algorithm::Rc4_128);
}

#[test]
fn test_v4_cfm_v2_roundtrip() {
    let original = EncryptDict {
        filter: "Standard".to_string(),
        sub_filter: None,
        version: 4,
        length: Some(128),
        revision: 4,
        owner_password: vec![5u8; 32],
        user_password: vec![6u8; 32],
        permissions: -1580,
        encrypt_metadata: false,
        owner_encryption: None,
        user_encryption: None,
        perms: None,
        stream_crypt_method: Some("V2".to_string()),
    };
    let obj = original.to_object();
    let parsed = EncryptDict::from_object(&obj).unwrap();
    assert_eq!(parsed.version, 4);
    assert_eq!(parsed.stream_crypt_method.as_deref(), Some("V2"));
    assert_eq!(parsed.algorithm().unwrap(), Algorithm::Rc4_128);
}
