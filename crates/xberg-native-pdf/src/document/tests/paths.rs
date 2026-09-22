use super::super::*;
use super::pdf_fixtures::*;

#[test]
fn test_oc_name_ocg_ascii() {
    let doc = oc_test_doc();
    let dict = ocg_dict(Object::String(b"A-GRID".to_vec()));
    assert_eq!(doc.read_oc_name(dict.as_dict().unwrap(), 8).as_deref(), Some("A-GRID"));
}

#[test]
fn test_oc_name_ocg_utf16be_bom() {
    let doc = oc_test_doc();
    let dict = ocg_dict(utf16_string("EJES", true));
    assert_eq!(doc.read_oc_name(dict.as_dict().unwrap(), 8).as_deref(), Some("EJES"));
}

#[test]
fn test_oc_name_ocmd_single_ocg() {
    // OCMD has no /Name — resolution follows /OCGs (single OCG) to its name. ~keep
    let doc = oc_test_doc();
    let ocmd = ocmd_dict(ocg_dict(Object::String(b"M-DUCT".to_vec())));
    assert_eq!(doc.read_oc_name(ocmd.as_dict().unwrap(), 8).as_deref(), Some("M-DUCT"));
}

#[test]
fn test_oc_name_ocmd_ocgs_array_first_wins() {
    // /OCGs may be an array of OCGs; the first resolvable member wins. ~keep
    let doc = oc_test_doc();
    let arr = Object::Array(vec![
        ocg_dict(Object::String(b"S-COLS".to_vec())),
        ocg_dict(Object::String(b"S-BEAM".to_vec())),
    ]);
    let ocmd = ocmd_dict(arr);
    assert_eq!(doc.read_oc_name(ocmd.as_dict().unwrap(), 8).as_deref(), Some("S-COLS"));
}

#[test]
fn test_oc_name_ocmd_depth_guard() {
    // A pathological OCMD chain (each /OCGs points to another OCMD) must
    // terminate via the depth guard rather than recursing without bound. ~keep
    let doc = oc_test_doc();
    let mut nested = ocmd_dict(Object::Array(vec![]));
    for _ in 0..20 {
        nested = ocmd_dict(nested);
    }
    assert_eq!(doc.read_oc_name(nested.as_dict().unwrap(), 8), None);
}

#[test]
fn test_resolve_oc_name_via_resources_properties() {
    // Case 2 (name reference) resolves against the *passed-in* resources.
    // This is the crux of the Form-XObject fix: the resolver reads
    // /Properties /<name> from whatever resource scope the caller hands
    // it — page /Resources at page level, the XObject's own /Resources
    // when extracting inside a Form XObject. ~keep
    let doc = oc_test_doc();
    let mut props = std::collections::HashMap::new();
    props.insert("MC0".to_string(), ocg_dict(Object::String(b"A-WALL-DIM".to_vec())));
    let mut resources = std::collections::HashMap::new();
    resources.insert("Properties".to_string(), Object::Dictionary(props));
    let resources = Object::Dictionary(resources);

    let name_ref = Object::Name("MC0".to_string());
    assert_eq!(
        doc.resolve_oc_layer_name(Some(&resources), &name_ref).as_deref(),
        Some("A-WALL-DIM")
    );
}

#[test]
fn test_resolve_oc_name_inline_dict() {
    let doc = oc_test_doc();
    let inline = ocg_dict(Object::String(b"CORTES".to_vec()));
    assert_eq!(doc.resolve_oc_layer_name(None, &inline).as_deref(), Some("CORTES"));
}

#[test]
fn test_resolve_oc_name_unresolvable_is_none() {
    // A name reference with no resources in scope yields None (the path
    // is left unlabelled) rather than an error. ~keep
    let doc = oc_test_doc();
    let name_ref = Object::Name("MC9".to_string());
    assert_eq!(doc.resolve_oc_layer_name(None, &name_ref), None);
}
